use core::cell::UnsafeCell;
use arch::aarch64::context::Aarch64Context;
use arch::aarch64::smp;
use arch::context::Context;
use crate::kprintln;
use crate::spinlock::SpinLock;

use crate::os_cfg;

const MAX_TASKS: usize = os_cfg::MAX_TASKS;
const MAX_PRIO: usize = os_cfg::PRIO_LEVELS;
const TIMESLICE_TICKS: u32 = os_cfg::TIMESLICE_TICKS;
const IDLE_STACK_SIZE: usize = os_cfg::IDLE_STACK_SIZE;
pub const MAX_CORES: usize = smp::MAX_CORES;

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum TaskState {
    Dormant = 0,
    Ready = 1,
    Running = 2,
    Blocked = 3,
    Suspended = 4,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WaitResult {
    Ok,
    Timeout,
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum Criticality {
    SafetyCritical = 0,
    MissionCritical = 1,
    Standard = 2,
    BestEffort = 3,
}

impl Criticality {
    pub fn as_str(self) -> &'static str {
        match self {
            Criticality::SafetyCritical => "safety",
            Criticality::MissionCritical => "mission",
            Criticality::Standard => "standard",
            Criticality::BestEffort => "best-ef",
        }
    }
}

const STACK_CANARY: u8 = 0xAA;

pub const CAP_YIELD: u32    = 1 << 0;
pub const CAP_DELAY: u32    = 1 << 1;
pub const CAP_WRITE: u32    = 1 << 2;
pub const CAP_TASKID: u32   = 1 << 3;
pub const CAP_UPTIME: u32   = 1 << 4;
pub const CAP_EXIT: u32     = 1 << 5;
pub const CAP_TEMP: u32     = 1 << 6;
pub const CAP_FS: u32       = 1 << 10;
pub const CAP_NET: u32      = 1 << 11;
pub const CAP_SPI: u32      = 1 << 12;
pub const CAP_I2C: u32      = 1 << 13;
pub const CAP_GPIO: u32     = 1 << 14;
pub const CAP_ALL: u32      = 0xFFFFFFFF;
pub const CAP_USER_DEFAULT: u32 = CAP_YIELD | CAP_DELAY | CAP_WRITE | CAP_TASKID
    | CAP_UPTIME | CAP_EXIT | CAP_TEMP | CAP_FS | CAP_NET;

#[repr(C)]
pub struct Tcb {
    pub sp: u64,
    pub id: u8,
    pub priority: u8,
    pub base_priority: u8,
    pub state: TaskState,
    pub wait_result: WaitResult,
    pub criticality: Criticality,
    pub core: u8,
    pub is_user: bool,
    pub ticks_remaining: u32,
    pub delay_ticks: u32,
    pub budget_ticks: u32,
    pub budget_remaining: u32,
    pub budget_period: u32,
    pub budget_replenish_at: u64,
    pub total_run_ticks: u64,
    pub ttbr0: u64,
    pub stack_base: usize,
    pub stack_size: usize,
    pub capabilities: u32,
    pub name: &'static str,
    pub next: u8,
}

impl Tcb {
    const fn empty() -> Self {
        Self {
            sp: 0,
            id: 0,
            priority: 255,
            base_priority: 255,
            state: TaskState::Dormant,
            wait_result: WaitResult::Ok,
            criticality: Criticality::Standard,
            core: 0xFF,
            is_user: false,
            ticks_remaining: 0,
            delay_ticks: 0,
            budget_ticks: 0,
            budget_remaining: 0,
            budget_period: 0,
            budget_replenish_at: 0,
            total_run_ticks: 0,
            ttbr0: 0,
            stack_base: 0,
            stack_size: 0,
            capabilities: 0xFFFFFFFF,
            name: "",
            next: 0xFF,
        }
    }
}

struct ReadyQueue {
    head: u8,
    tail: u8,
}

impl ReadyQueue {
    const fn empty() -> Self {
        Self {
            head: 0xFF,
            tail: 0xFF,
        }
    }
}

struct Scheduler {
    tasks: [Tcb; MAX_TASKS],
    ready: [ReadyQueue; MAX_PRIO],
    prio_bitmap: [u64; 4],
    current: [u8; MAX_CORES],
    idle_task: [u8; MAX_CORES],
    task_count: u8,
    started: bool,
    num_cores: u8,
    total_ticks: u64,
    idle_ticks: u64,
}

impl Scheduler {
    const fn new() -> Self {
        const EMPTY_TCB: Tcb = Tcb::empty();
        const EMPTY_Q: ReadyQueue = ReadyQueue::empty();
        Self {
            tasks: [EMPTY_TCB; MAX_TASKS],
            ready: [EMPTY_Q; MAX_PRIO],
            prio_bitmap: [0; 4],
            current: [0xFF; MAX_CORES],
            idle_task: [0xFF; MAX_CORES],
            task_count: 0,
            started: false,
            num_cores: 1,
            total_ticks: 0,
            idle_ticks: 0,
        }
    }

    fn alloc_id(&mut self) -> Option<u8> {
        for i in 0..MAX_TASKS {
            if self.tasks[i].state == TaskState::Dormant {
                return Some(i as u8);
            }
        }
        None
    }

    fn enqueue(&mut self, id: u8) {
        let prio = self.tasks[id as usize].priority as usize;
        self.tasks[id as usize].next = 0xFF;

        let q = &mut self.ready[prio];
        if q.tail != 0xFF {
            self.tasks[q.tail as usize].next = id;
        } else {
            q.head = id;
        }
        q.tail = id;
        self.prio_bitmap[prio / 64] |= 1 << (prio % 64);
    }

    fn dequeue_highest(&mut self) -> Option<u8> {
        let prio = self.find_highest_prio()?;
        let q = &mut self.ready[prio];
        let id = q.head;
        if id == 0xFF {
            return None;
        }
        q.head = self.tasks[id as usize].next;
        if q.head == 0xFF {
            q.tail = 0xFF;
            self.prio_bitmap[prio / 64] &= !(1 << (prio % 64));
        }
        self.tasks[id as usize].next = 0xFF;
        Some(id)
    }

    fn find_highest_prio(&self) -> Option<usize> {
        for word_idx in 0..4 {
            let word = self.prio_bitmap[word_idx];
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                return Some(word_idx * 64 + bit);
            }
        }
        None
    }
}

// Global scheduler protected by a spinlock.
static SCHED_LOCK: SpinLock = SpinLock::new();

struct SchedCell(UnsafeCell<Scheduler>);
unsafe impl Sync for SchedCell {}

static SCHED: SchedCell = SchedCell(UnsafeCell::new(Scheduler::new()));

/// Access the scheduler internals. Caller MUST hold SCHED_LOCK.
fn sched() -> &'static mut Scheduler {
    // SAFETY: Protected by SCHED_LOCK spinlock.
    unsafe { &mut *SCHED.0.get() }
}

// Per-core idle task stacks.
#[repr(align(16))]
struct IdleStack([u8; IDLE_STACK_SIZE]);
static mut IDLE_STACKS: [IdleStack; MAX_CORES] = [
    IdleStack([0; IDLE_STACK_SIZE]),
    IdleStack([0; IDLE_STACK_SIZE]),
    IdleStack([0; IDLE_STACK_SIZE]),
    IdleStack([0; IDLE_STACK_SIZE]),
];

// Throwaway slot for discarding a terminated task's context during context_switch.
static mut DISCARD_SP: u64 = 0;

fn idle_entry(_arg: usize) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}

/// Critical section guard. Masks IRQs on construction, restores on drop.
/// Still useful for protecting non-scheduler state on a single core.
pub(crate) struct CriticalSection {
    daif: u64,
}

impl CriticalSection {
    pub(crate) fn enter() -> Self {
        let daif: u64;
        unsafe {
            core::arch::asm!("mrs {}, daif", out(reg) daif);
            core::arch::asm!("msr daifset, #2");
        }
        CriticalSection { daif }
    }
}

impl Drop for CriticalSection {
    fn drop(&mut self) {
        if self.daif & (1 << 7) == 0 {
            unsafe { core::arch::asm!("msr daifclr, #2") };
        }
    }
}

/// Called by task_trampoline (assembly) when a newly created task runs
/// for the first time. Releases the scheduler lock that was held by the
/// schedule() or start() call that switched to this task.
#[no_mangle]
pub extern "C" fn sched_unlock_new_task() {
    // SAFETY: The lock is held by the code that context-switched to this task.
    unsafe { SCHED_LOCK.force_unlock() };
}

/// Initialize the scheduler and create the idle task for core 0.
pub fn init() {
    let saved = SCHED_LOCK.lock();
    let s = sched();

    create_idle_task(s, 0);
    s.task_count = 1;

    SCHED_LOCK.unlock(saved);
}

/// Create an idle task for the given core. Caller must hold SCHED_LOCK.
fn create_idle_task(s: &mut Scheduler, core: usize) {
    let id = s.alloc_id().expect("no TCB slots for idle task");

    let idle_stack = unsafe { &mut IDLE_STACKS[core].0[..] };
    for byte in idle_stack.iter_mut() {
        *byte = STACK_CANARY;
    }

    let stack_base = unsafe { (&raw const IDLE_STACKS[core].0) as usize };
    let stack_top = unsafe {
        ((&raw mut IDLE_STACKS[core].0) as *mut u8).add(IDLE_STACK_SIZE)
    };
    let sp = Aarch64Context::new_context(idle_entry, 0, stack_top);
    s.tasks[id as usize] = Tcb {
        sp,
        id,
        priority: 255,
        base_priority: 255,
        state: TaskState::Ready,
        wait_result: WaitResult::Ok,
        criticality: Criticality::BestEffort,
        core: core as u8,
        is_user: false,
        ticks_remaining: 0,
        delay_ticks: 0,
        budget_ticks: 0,
        budget_remaining: 0,
        budget_period: 0,
        budget_replenish_at: 0,
        total_run_ticks: 0,
        ttbr0: 0,
        stack_base,
        stack_size: IDLE_STACK_SIZE,
        capabilities: CAP_ALL,
        name: match core {
            0 => "idle-0",
            1 => "idle-1",
            2 => "idle-2",
            3 => "idle-3",
            _ => "idle-?",
        },
        next: 0xFF,
    };
    s.enqueue(id);
    s.idle_task[core] = id;
}

/// Create a new task. Returns the task ID.
pub fn task_create(
    name: &'static str,
    priority: u8,
    criticality: Criticality,
    stack: &'static mut [u8],
    entry: fn(usize) -> !,
    arg: usize,
) -> Result<u8, &'static str> {
    let saved = SCHED_LOCK.lock();
    let s = sched();

    let id = s.alloc_id().ok_or("no TCB slots available")?;

    let stack_base = stack.as_ptr() as usize;
    let stack_size = stack.len();
    for byte in stack.iter_mut() {
        *byte = STACK_CANARY;
    }

    let stack_top = unsafe { stack.as_mut_ptr().add(stack.len()) };
    let sp = Aarch64Context::new_context(entry, arg, stack_top);

    s.tasks[id as usize] = Tcb {
        sp,
        id,
        priority,
        base_priority: priority,
        state: TaskState::Ready,
        wait_result: WaitResult::Ok,
        criticality,
        core: 0xFF,
        is_user: false,
        ticks_remaining: TIMESLICE_TICKS,
        delay_ticks: 0,
        budget_ticks: 0,
        budget_remaining: 0,
        budget_period: 0,
        budget_replenish_at: 0,
        total_run_ticks: 0,
        ttbr0: 0,
        stack_base,
        stack_size,
        capabilities: CAP_ALL,
        name,
        next: 0xFF,
    };
    s.enqueue(id);
    s.task_count += 1;

    if s.started {
        let core = smp::core_id();
        let cur = s.current[core];
        if cur != 0xFF {
            let cur_prio = s.tasks[cur as usize].priority;
            if priority < cur_prio {
                schedule_locked(s, core);
                SCHED_LOCK.unlock(saved);
                return Ok(id);
            }
        }
        // Check if any other core is idle and could pick up this task.
        send_ipi_to_idle_core(s, core);
    }

    SCHED_LOCK.unlock(saved);
    Ok(id)
}

pub fn task_create_user(
    name: &'static str,
    priority: u8,
    criticality: Criticality,
    kernel_stack: &'static mut [u8],
    user_entry: usize,
    user_stack_top: usize,
    arg: usize,
    ttbr0: u64,
) -> Result<u8, &'static str> {
    let saved = SCHED_LOCK.lock();
    let s = sched();

    let id = s.alloc_id().ok_or("no TCB slots available")?;

    let stack_base = kernel_stack.as_ptr() as usize;
    let stack_size = kernel_stack.len();
    for byte in kernel_stack.iter_mut() {
        *byte = STACK_CANARY;
    }

    let kernel_stack_top = unsafe { kernel_stack.as_mut_ptr().add(kernel_stack.len()) };
    let sp = Aarch64Context::new_user_context(
        user_entry, user_stack_top, arg, kernel_stack_top,
    );

    s.tasks[id as usize] = Tcb {
        sp,
        id,
        priority,
        base_priority: priority,
        state: TaskState::Ready,
        wait_result: WaitResult::Ok,
        criticality,
        core: 0xFF,
        is_user: true,
        ticks_remaining: TIMESLICE_TICKS,
        delay_ticks: 0,
        budget_ticks: 0,
        budget_remaining: 0,
        budget_period: 0,
        budget_replenish_at: 0,
        total_run_ticks: 0,
        ttbr0,
        stack_base,
        stack_size,
        capabilities: CAP_USER_DEFAULT,
        name,
        next: 0xFF,
    };
    s.enqueue(id);
    s.task_count += 1;

    if s.started {
        send_ipi_to_idle_core(s, smp::core_id());
    }

    SCHED_LOCK.unlock(saved);
    Ok(id)
}

pub fn task_terminate(id: u8) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state == TaskState::Dormant {
        SCHED_LOCK.unlock(saved);
        return;
    }

    let core = smp::core_id();
    let is_current = s.current[core] == id;

    if s.tasks[idx].ttbr0 != 0 {
        if is_current {
            unsafe { arch::aarch64::mmu::switch_ttbr0(arch::aarch64::mmu::kernel_ttbr0()); }
        }
        unsafe { arch::aarch64::mmu::free_user_page_table(s.tasks[idx].ttbr0); }
        s.tasks[idx].ttbr0 = 0;
    }

    s.tasks[idx].state = TaskState::Dormant;
    s.tasks[idx].is_user = false;
    s.task_count -= 1;

    if is_current {
        s.current[core] = 0xFF;
        schedule_locked(s, core);
    }
    SCHED_LOCK.unlock(saved);
}

/// Suspend a task.
pub fn task_suspend(id: u8) -> Result<(), &'static str> {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state == TaskState::Dormant {
        SCHED_LOCK.unlock(saved);
        return Err("invalid task id");
    }
    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
    }
    s.tasks[idx].state = TaskState::Suspended;

    let core = smp::core_id();
    if s.current[core] == id {
        schedule_locked(s, core);
        SCHED_LOCK.unlock(saved);
        return Ok(());
    }
    SCHED_LOCK.unlock(saved);
    Ok(())
}

/// Resume a suspended task.
pub fn task_resume(id: u8) -> Result<(), &'static str> {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state != TaskState::Suspended {
        SCHED_LOCK.unlock(saved);
        return Err("task not suspended");
    }
    s.tasks[idx].state = TaskState::Ready;
    s.enqueue(id);

    let core = smp::core_id();
    if s.started && s.current[core] != 0xFF {
        let cur_prio = s.tasks[s.current[core] as usize].priority;
        if s.tasks[idx].priority < cur_prio {
            schedule_locked(s, core);
            SCHED_LOCK.unlock(saved);
            return Ok(());
        }
        send_ipi_to_idle_core(s, core);
    }
    SCHED_LOCK.unlock(saved);
    Ok(())
}

/// Delete a task, releasing its TCB slot.
pub fn task_delete(id: u8) -> Result<(), &'static str> {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state == TaskState::Dormant {
        SCHED_LOCK.unlock(saved);
        return Err("invalid task id");
    }
    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
    }
    s.tasks[idx].state = TaskState::Dormant;
    s.task_count -= 1;

    let core = smp::core_id();
    if s.current[core] == id {
        s.current[core] = 0xFF;
        schedule_locked(s, core);
        SCHED_LOCK.unlock(saved);
        return Ok(());
    }
    SCHED_LOCK.unlock(saved);
    Ok(())
}

/// Yield the current task's remaining timeslice.
pub fn task_yield() {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    if s.current[core] != 0xFF {
        let idx = s.current[core] as usize;
        s.tasks[idx].ticks_remaining = TIMESLICE_TICKS;
    }
    schedule_locked(s, core);
    SCHED_LOCK.unlock(saved);
}

/// Block the current task for `ticks` timer ticks.
pub fn delay(ticks: u32) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    if s.current[core] == 0xFF || !s.started {
        SCHED_LOCK.unlock(saved);
        return;
    }
    let idx = s.current[core] as usize;
    s.tasks[idx].delay_ticks = ticks;
    s.tasks[idx].state = TaskState::Blocked;
    schedule_locked(s, core);
    SCHED_LOCK.unlock(saved);
}

/// Start the scheduler on the primary core. Does not return.
pub fn start() -> ! {
    let _saved = SCHED_LOCK.lock();
    let s = sched();
    s.started = true;
    s.num_cores = 1;

    let id = s.dequeue_highest().expect("no tasks to run");
    s.current[0] = id;
    s.tasks[id as usize].state = TaskState::Running;
    s.tasks[id as usize].ticks_remaining = TIMESLICE_TICKS;
    s.tasks[id as usize].core = 0;

    let sp = s.tasks[id as usize].sp;
    kprintln!("sched: starting '{}' (id={}, prio={}) on core 0",
        s.tasks[id as usize].name, id, s.tasks[id as usize].priority);

    // Lock is NOT released here — task_trampoline calls sched_unlock_new_task().
    unsafe {
        core::arch::asm!(
            "mov sp, {0}",
            "ldp x29, x30, [sp, #80]",
            "ldp x27, x28, [sp, #64]",
            "ldp x25, x26, [sp, #48]",
            "ldp x23, x24, [sp, #32]",
            "ldp x21, x22, [sp, #16]",
            "ldp x19, x20, [sp], #96",
            "ret",
            in(reg) sp,
            options(noreturn),
        );
    }
}

/// Start the scheduler on a secondary core. Called from secondary_main().
pub fn start_secondary(core: usize) -> ! {
    let _saved = SCHED_LOCK.lock();
    let s = sched();

    create_idle_task(s, core);
    s.task_count += 1;
    if core as u8 + 1 > s.num_cores {
        s.num_cores = core as u8 + 1;
    }

    let id = s.dequeue_highest().expect("no tasks for secondary core");
    s.current[core] = id;
    s.tasks[id as usize].state = TaskState::Running;
    s.tasks[id as usize].ticks_remaining = TIMESLICE_TICKS;
    s.tasks[id as usize].core = core as u8;

    let sp = s.tasks[id as usize].sp;
    kprintln!("sched: core {} starting '{}' (id={}, prio={})",
        core, s.tasks[id as usize].name, id, s.tasks[id as usize].priority);

    // Lock released by task_trampoline → sched_unlock_new_task().
    unsafe {
        core::arch::asm!(
            "mov sp, {0}",
            "ldp x29, x30, [sp, #80]",
            "ldp x27, x28, [sp, #64]",
            "ldp x25, x26, [sp, #48]",
            "ldp x23, x24, [sp, #32]",
            "ldp x21, x22, [sp, #16]",
            "ldp x19, x20, [sp], #96",
            "ret",
            in(reg) sp,
            options(noreturn),
        );
    }
}

/// Called from each core's timer tick ISR.
pub fn tick() {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();

    if !s.started || s.current[core] == 0xFF {
        SCHED_LOCK.unlock(saved);
        return;
    }

    // Global utilization tracking.
    s.total_ticks += 1;
    let idx = s.current[core] as usize;
    s.tasks[idx].total_run_ticks += 1;
    if s.tasks[idx].priority == 255 {
        s.idle_ticks += 1;
    }

    let mut woke_higher = false;
    let cur_prio = s.tasks[idx].priority;

    // Budget enforcement: suspend task on overrun, call hook.
    if s.tasks[idx].budget_ticks > 0 && s.tasks[idx].budget_remaining > 0 {
        s.tasks[idx].budget_remaining -= 1;
        if s.tasks[idx].budget_remaining == 0 {
            crate::klog_warn!("sched", "task '{}' (id={}) budget exhausted",
                s.tasks[idx].name, s.tasks[idx].id);
            crate::hooks::os_hook_budget_overrun(s.tasks[idx].id, s.tasks[idx].name);
            if os_cfg::BUDGET_EN {
                s.tasks[idx].state = TaskState::Suspended;
                s.tasks[idx].core = 0xFF;
                if s.tasks[idx].budget_period > 0 {
                    s.tasks[idx].budget_replenish_at =
                        s.total_ticks + s.tasks[idx].budget_period as u64;
                }
                woke_higher = true;
            }
        }
    }

    // Budget replenishment: re-ready suspended tasks whose period has elapsed.
    for i in 0..MAX_TASKS {
        if s.tasks[i].state == TaskState::Suspended
            && s.tasks[i].budget_period > 0
            && s.tasks[i].budget_replenish_at > 0
            && s.total_ticks >= s.tasks[i].budget_replenish_at
        {
            s.tasks[i].budget_remaining = s.tasks[i].budget_ticks;
            s.tasks[i].budget_replenish_at = 0;
            s.tasks[i].state = TaskState::Ready;
            s.enqueue(i as u8);
            if s.tasks[i].priority < cur_prio {
                woke_higher = true;
            }
        }
    }

    // Software watchdog (runs on core 0 only).
    if core == 0 {
        crate::watchdog::tick();
    }

    // Check all blocked tasks for delay expiry.
    for i in 0..MAX_TASKS {
        if s.tasks[i].state == TaskState::Blocked && s.tasks[i].delay_ticks > 0 {
            s.tasks[i].delay_ticks -= 1;
            if s.tasks[i].delay_ticks == 0 {
                s.tasks[i].wait_result = WaitResult::Timeout;
                s.tasks[i].state = TaskState::Ready;
                s.enqueue(i as u8);
                if s.tasks[i].priority < cur_prio {
                    woke_higher = true;
                }
            }
        }
    }

    if s.tasks[idx].ticks_remaining > 0 {
        s.tasks[idx].ticks_remaining -= 1;
    }

    if woke_higher || s.tasks[idx].ticks_remaining == 0 {
        schedule_locked(s, core);
    }

    SCHED_LOCK.unlock(saved);
}

/// Handle an IPI reschedule interrupt on this core.
pub fn ipi_reschedule() {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    schedule_locked(s, core);
    SCHED_LOCK.unlock(saved);
}

/// Core scheduling logic. Caller must hold SCHED_LOCK.
/// After context_switch, the RETURNED-TO task resumes here and the
/// caller releases the lock.
fn swap_ttbr0_if_needed(cur_ttbr0: u64, next_ttbr0: u64) {
    if next_ttbr0 != cur_ttbr0 {
        let ttbr = if next_ttbr0 != 0 {
            next_ttbr0
        } else {
            arch::aarch64::mmu::kernel_ttbr0()
        };
        unsafe { arch::aarch64::mmu::switch_ttbr0(ttbr); }
    }
}

fn schedule_locked(s: &mut Scheduler, core: usize) {
    if !s.started {
        return;
    }

    let cur = s.current[core];
    if cur == 0xFF {
        if let Some(next_id) = s.dequeue_highest() {
            s.current[core] = next_id;
            s.tasks[next_id as usize].state = TaskState::Running;
            s.tasks[next_id as usize].ticks_remaining = TIMESLICE_TICKS;
            s.tasks[next_id as usize].core = core as u8;

            swap_ttbr0_if_needed(0, s.tasks[next_id as usize].ttbr0);

            let discard_ptr = unsafe { &raw mut DISCARD_SP };
            let next_sp_ptr = &s.tasks[next_id as usize].sp as *const u64;
            unsafe { Aarch64Context::switch(discard_ptr, next_sp_ptr) };
        }
        return;
    }

    let cur_idx = cur as usize;
    let cur_prio = s.tasks[cur_idx].priority;
    let cur_runnable = s.tasks[cur_idx].state == TaskState::Running;

    if !cur_runnable {
        if let Some(next_id) = s.dequeue_highest() {
            s.current[core] = next_id;
            s.tasks[next_id as usize].state = TaskState::Running;
            s.tasks[next_id as usize].ticks_remaining = TIMESLICE_TICKS;
            s.tasks[next_id as usize].core = core as u8;
            s.tasks[cur_idx].core = 0xFF;

            swap_ttbr0_if_needed(s.tasks[cur_idx].ttbr0, s.tasks[next_id as usize].ttbr0);
            let cur_sp_ptr = &mut s.tasks[cur_idx].sp as *mut u64;
            let next_sp_ptr = &s.tasks[next_id as usize].sp as *const u64;
            unsafe { Aarch64Context::switch(cur_sp_ptr, next_sp_ptr) };
        }
        return;
    }

    let next_prio = s.find_highest_prio();
    let should_switch = match next_prio {
        Some(p) => p < cur_prio as usize
            || (p == cur_prio as usize && s.tasks[cur_idx].ticks_remaining == 0),
        None => false,
    };

    if !should_switch {
        if s.tasks[cur_idx].ticks_remaining == 0 {
            s.tasks[cur_idx].ticks_remaining = TIMESLICE_TICKS;
        }
        return;
    }

    let next_id = match s.dequeue_highest() {
        Some(id) => id,
        None => return,
    };

    if next_id == cur {
        s.tasks[cur_idx].ticks_remaining = TIMESLICE_TICKS;
        s.tasks[cur_idx].state = TaskState::Running;
        return;
    }

    s.tasks[cur_idx].state = TaskState::Ready;
    s.tasks[cur_idx].core = 0xFF;
    s.enqueue(cur);

    let next_idx = next_id as usize;
    s.current[core] = next_id;
    s.tasks[next_idx].state = TaskState::Running;
    s.tasks[next_idx].ticks_remaining = TIMESLICE_TICKS;
    s.tasks[next_idx].core = core as u8;

    swap_ttbr0_if_needed(s.tasks[cur_idx].ttbr0, s.tasks[next_idx].ttbr0);
    let cur_sp_ptr = &mut s.tasks[cur_idx].sp as *mut u64;
    let next_sp_ptr = &s.tasks[next_idx].sp as *const u64;

    unsafe { Aarch64Context::switch(cur_sp_ptr, next_sp_ptr) };
}

/// Send an IPI to one idle core (if any) so it picks up ready work.
fn send_ipi_to_idle_core(s: &Scheduler, exclude_core: usize) {
    use arch::aarch64::gic;

    for c in 0..s.num_cores as usize {
        if c == exclude_core {
            continue;
        }
        let cur = s.current[c];
        if cur != 0xFF && s.tasks[cur as usize].priority == 255 {
            gic::send_sgi(c as u8, gic::SGI_RESCHEDULE as u8);
            return;
        }
    }
}

fn remove_from_ready(s: &mut Scheduler, id: u8) {
    let prio = s.tasks[id as usize].priority as usize;
    let q = &mut s.ready[prio];

    if q.head == id {
        q.head = s.tasks[id as usize].next;
        if q.head == 0xFF {
            q.tail = 0xFF;
            s.prio_bitmap[prio / 64] &= !(1 << (prio % 64));
        }
        s.tasks[id as usize].next = 0xFF;
        return;
    }

    let mut prev = q.head;
    while prev != 0xFF {
        let next = s.tasks[prev as usize].next;
        if next == id {
            s.tasks[prev as usize].next = s.tasks[id as usize].next;
            if q.tail == id {
                q.tail = prev;
            }
            s.tasks[id as usize].next = 0xFF;
            if q.head == 0xFF {
                s.prio_bitmap[prio / 64] &= !(1 << (prio % 64));
            }
            return;
        }
        prev = next;
    }
}

// --- Public APIs for sync primitives ---

pub fn current_id() -> u8 {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    let id = s.current[core];
    SCHED_LOCK.unlock(saved);
    id
}

pub fn get_priority(id: u8) -> u8 {
    sched().tasks[id as usize].priority
}

pub fn get_base_priority(id: u8) -> u8 {
    sched().tasks[id as usize].base_priority
}

pub fn get_state(id: u8) -> TaskState {
    sched().tasks[id as usize].state
}

pub fn get_wait_result() -> WaitResult {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    let cur = s.current[core];
    let result = if cur != 0xFF {
        s.tasks[cur as usize].wait_result
    } else {
        WaitResult::Ok
    };
    SCHED_LOCK.unlock(saved);
    result
}

pub fn set_task_wait_result(id: u8, result: WaitResult) {
    sched().tasks[id as usize].wait_result = result;
}

/// Block the current task indefinitely (until explicitly woken).
pub fn block_current() {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    if s.current[core] == 0xFF || !s.started {
        SCHED_LOCK.unlock(saved);
        return;
    }
    let idx = s.current[core] as usize;
    s.tasks[idx].delay_ticks = 0;
    s.tasks[idx].state = TaskState::Blocked;
    schedule_locked(s, core);
    SCHED_LOCK.unlock(saved);
}

/// Block the current task with a timeout in ticks. 0 = infinite wait.
pub fn block_current_timeout(ticks: u32) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    if s.current[core] == 0xFF || !s.started {
        SCHED_LOCK.unlock(saved);
        return;
    }
    let idx = s.current[core] as usize;
    s.tasks[idx].delay_ticks = ticks;
    s.tasks[idx].state = TaskState::Blocked;
    schedule_locked(s, core);
    SCHED_LOCK.unlock(saved);
}

/// Wake a blocked task and make it ready.
pub fn wake_task(id: u8) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state != TaskState::Blocked {
        SCHED_LOCK.unlock(saved);
        return;
    }
    s.tasks[idx].state = TaskState::Ready;
    s.tasks[idx].delay_ticks = 0;
    s.enqueue(id);

    let core = smp::core_id();
    if s.started && s.current[core] != 0xFF {
        let cur_prio = s.tasks[s.current[core] as usize].priority;
        if s.tasks[idx].priority < cur_prio {
            schedule_locked(s, core);
            SCHED_LOCK.unlock(saved);
            return;
        }
        send_ipi_to_idle_core(s, core);
    }
    SCHED_LOCK.unlock(saved);
}

/// Change a task's effective priority (for PIP/PCP).
pub fn set_priority(id: u8, prio: u8) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS {
        SCHED_LOCK.unlock(saved);
        return;
    }
    let old_prio = s.tasks[idx].priority;
    if prio == old_prio {
        SCHED_LOCK.unlock(saved);
        return;
    }

    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
        s.tasks[idx].priority = prio;
        s.enqueue(id);
    } else {
        s.tasks[idx].priority = prio;
    }
    SCHED_LOCK.unlock(saved);
}

/// Return info about all non-dormant tasks for the shell.
pub fn task_list() -> [(u8, &'static str, u8, TaskState); MAX_TASKS] {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let mut result = [(0u8, "", 0u8, TaskState::Dormant); MAX_TASKS];
    for i in 0..MAX_TASKS {
        if s.tasks[i].state != TaskState::Dormant {
            result[i] = (
                s.tasks[i].id,
                s.tasks[i].name,
                s.tasks[i].priority,
                s.tasks[i].state,
            );
        }
    }
    SCHED_LOCK.unlock(saved);
    result
}

pub fn current_task_name() -> &'static str {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    let name = if s.current[core] != 0xFF && s.started {
        s.tasks[s.current[core] as usize].name
    } else {
        "none"
    };
    SCHED_LOCK.unlock(saved);
    name
}

pub fn task_count() -> u8 {
    sched().task_count
}

pub fn current_task_id() -> u8 {
    let core = smp::core_id();
    sched().current[core]
}

pub fn task_has_capability(cap: u32) -> bool {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let core = smp::core_id();
    let id = s.current[core] as usize;
    let result = (s.tasks[id].capabilities & cap) != 0;
    SCHED_LOCK.unlock(saved);
    result
}

pub fn task_capabilities(task_id: u8) -> u32 {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let caps = s.tasks[task_id as usize].capabilities;
    SCHED_LOCK.unlock(saved);
    caps
}

pub fn active_cores() -> u8 {
    sched().num_cores
}

// --- Budget and criticality APIs ---

pub fn task_set_budget(id: u8, ticks: u32) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS && s.tasks[idx].state != TaskState::Dormant {
        s.tasks[idx].budget_ticks = ticks;
        s.tasks[idx].budget_remaining = ticks;
    }
    SCHED_LOCK.unlock(saved);
}

pub fn task_get_remaining(id: u8) -> u32 {
    sched().tasks[id as usize].budget_remaining
}

pub fn task_reset_budget(id: u8) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS {
        s.tasks[idx].budget_remaining = s.tasks[idx].budget_ticks;
    }
    SCHED_LOCK.unlock(saved);
}

pub fn task_set_budget_period(id: u8, period_ticks: u32) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS && s.tasks[idx].state != TaskState::Dormant {
        s.tasks[idx].budget_period = period_ticks;
    }
    SCHED_LOCK.unlock(saved);
}

pub fn task_set_criticality(id: u8, crit: Criticality) {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS && s.tasks[idx].state != TaskState::Dormant {
        s.tasks[idx].criticality = crit;
    }
    SCHED_LOCK.unlock(saved);
}

// --- Utilization ---

pub fn utilization() -> (u64, u64) {
    let s = sched();
    let busy = s.total_ticks - s.idle_ticks;
    (busy, s.total_ticks)
}

pub fn tick_count_32() -> u32 {
    arch::aarch64::exceptions::tick_count() as u32
}

// --- Stack watermark ---

fn stack_watermark(base: usize, size: usize) -> usize {
    let ptr = base as *const u8;
    let mut unused = 0usize;
    for i in 0..size {
        if unsafe { *ptr.add(i) } == STACK_CANARY {
            unused += 1;
        } else {
            break;
        }
    }
    size - unused
}

/// Return stack info for all non-dormant tasks.
pub fn task_stack_info() -> [(u8, &'static str, usize, usize); MAX_TASKS] {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let mut result = [(0u8, "", 0usize, 0usize); MAX_TASKS];
    for i in 0..MAX_TASKS {
        if s.tasks[i].state != TaskState::Dormant && s.tasks[i].stack_size > 0 {
            let used = stack_watermark(s.tasks[i].stack_base, s.tasks[i].stack_size);
            result[i] = (s.tasks[i].id, s.tasks[i].name, used, s.tasks[i].stack_size);
        }
    }
    SCHED_LOCK.unlock(saved);
    result
}

/// Extended task list for shell display.
pub fn task_list_ext() -> [(u8, &'static str, u8, TaskState, Criticality, u32, u64); MAX_TASKS] {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let mut result = [(0u8, "", 0u8, TaskState::Dormant, Criticality::Standard, 0u32, 0u64); MAX_TASKS];
    for i in 0..MAX_TASKS {
        if s.tasks[i].state != TaskState::Dormant {
            result[i] = (
                s.tasks[i].id,
                s.tasks[i].name,
                s.tasks[i].priority,
                s.tasks[i].state,
                s.tasks[i].criticality,
                s.tasks[i].budget_ticks,
                s.tasks[i].total_run_ticks,
            );
        }
    }
    SCHED_LOCK.unlock(saved);
    result
}

// --- Health check helpers ---

pub fn check_ready_queue_integrity() -> bool {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let mut ok = true;
    for prio in 0..MAX_PRIO {
        let mut cursor = s.ready[prio].head;
        while cursor != 0xFF {
            let idx = cursor as usize;
            if idx >= MAX_TASKS || s.tasks[idx].state != TaskState::Ready {
                ok = false;
                break;
            }
            cursor = s.tasks[idx].next;
        }
    }
    SCHED_LOCK.unlock(saved);
    ok
}

pub fn check_mutex_ownership() -> bool {
    let saved = SCHED_LOCK.lock();
    let s = sched();
    let mut ok = true;
    for i in 0..MAX_TASKS {
        if s.tasks[i].state == TaskState::Dormant && s.tasks[i].priority != s.tasks[i].base_priority {
            ok = false;
        }
    }
    SCHED_LOCK.unlock(saved);
    ok
}

pub fn check_tick_monotonicity() -> bool {
    static mut LAST_TICK: u64 = 0;
    let current = arch::aarch64::exceptions::tick_count();
    // SAFETY: Only called from health monitor task (single caller).
    let ok = unsafe { current >= LAST_TICK };
    unsafe { LAST_TICK = current };
    ok
}
