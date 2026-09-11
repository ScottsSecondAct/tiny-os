use core::cell::UnsafeCell;
use arch::aarch64::context::Aarch64Context;
use arch::context::Context;
use crate::kprintln;

const MAX_TASKS: usize = 32;
const MAX_PRIO: usize = 256;
const TIMESLICE_TICKS: u32 = 10;
const IDLE_STACK_SIZE: usize = 4096;

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

#[repr(C)]
pub struct Tcb {
    pub sp: u64,
    pub id: u8,
    pub priority: u8,
    pub base_priority: u8,
    pub state: TaskState,
    pub wait_result: WaitResult,
    pub criticality: Criticality,
    pub ticks_remaining: u32,
    pub delay_ticks: u32,
    pub budget_ticks: u32,
    pub budget_remaining: u32,
    pub total_run_ticks: u64,
    pub stack_base: usize,
    pub stack_size: usize,
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
            ticks_remaining: 0,
            delay_ticks: 0,
            budget_ticks: 0,
            budget_remaining: 0,
            total_run_ticks: 0,
            stack_base: 0,
            stack_size: 0,
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
    current: u8,
    task_count: u8,
    started: bool,
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
            current: 0xFF,
            task_count: 0,
            started: false,
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
        // Lower numeric priority = higher urgency (0 is highest).
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

struct SchedCell(UnsafeCell<Scheduler>);
unsafe impl Sync for SchedCell {}

static SCHED: SchedCell = SchedCell(UnsafeCell::new(Scheduler::new()));

fn sched() -> &'static mut Scheduler {
    // SAFETY: Single-core, accessed with interrupts masked during critical sections.
    unsafe { &mut *SCHED.0.get() }
}

// Idle task stack in BSS.
#[repr(align(16))]
struct IdleStack([u8; IDLE_STACK_SIZE]);
static mut IDLE_STACK: IdleStack = IdleStack([0; IDLE_STACK_SIZE]);

fn idle_entry(_arg: usize) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}

/// Critical section guard. Masks IRQs on construction, restores on drop.
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

/// Initialize the scheduler and create the idle task.
pub fn init() {
    let s = sched();
    let id = s.alloc_id().expect("no TCB slots");

    // Fill idle stack with canary pattern.
    let idle_stack = unsafe { &mut IDLE_STACK.0[..] };
    for byte in idle_stack.iter_mut() {
        *byte = STACK_CANARY;
    }

    let stack_base = unsafe { (&raw const IDLE_STACK.0) as usize };
    let stack_top = unsafe {
        ((&raw mut IDLE_STACK.0) as *mut u8).add(IDLE_STACK_SIZE)
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
        ticks_remaining: 0,
        delay_ticks: 0,
        budget_ticks: 0,
        budget_remaining: 0,
        total_run_ticks: 0,
        stack_base,
        stack_size: IDLE_STACK_SIZE,
        name: "idle",
        next: 0xFF,
    };
    s.enqueue(id);
    s.task_count = 1;
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
    let _cs = CriticalSection::enter();
    let s = sched();

    let id = s.alloc_id().ok_or("no TCB slots available")?;

    // Fill stack with canary pattern for watermark tracking.
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
        ticks_remaining: TIMESLICE_TICKS,
        delay_ticks: 0,
        budget_ticks: 0,
        budget_remaining: 0,
        total_run_ticks: 0,
        stack_base,
        stack_size,
        name,
        next: 0xFF,
    };
    s.enqueue(id);
    s.task_count += 1;

    // If scheduler is running and new task has higher priority, request reschedule.
    if s.started && s.current != 0xFF {
        let cur_prio = s.tasks[s.current as usize].priority;
        if priority < cur_prio {
            // Will be picked up on next tick or explicit yield.
            schedule();
        }
    }

    Ok(id)
}

/// Suspend a task. It is removed from the ready queue (if there) and will
/// not be scheduled until resumed.
pub fn task_suspend(id: u8) -> Result<(), &'static str> {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state == TaskState::Dormant {
        return Err("invalid task id");
    }
    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
    }
    s.tasks[idx].state = TaskState::Suspended;

    if s.current == id {
        s.tasks[idx].state = TaskState::Suspended;
        schedule();
    }
    Ok(())
}

/// Resume a suspended task.
pub fn task_resume(id: u8) -> Result<(), &'static str> {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state != TaskState::Suspended {
        return Err("task not suspended");
    }
    s.tasks[idx].state = TaskState::Ready;
    s.enqueue(id);

    // If resumed task has higher priority than current, reschedule.
    if s.started && s.current != 0xFF {
        let cur_prio = s.tasks[s.current as usize].priority;
        if s.tasks[idx].priority < cur_prio {
            schedule();
        }
    }
    Ok(())
}

/// Delete a task, releasing its TCB slot.
pub fn task_delete(id: u8) -> Result<(), &'static str> {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state == TaskState::Dormant {
        return Err("invalid task id");
    }
    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
    }
    s.tasks[idx].state = TaskState::Dormant;
    s.task_count -= 1;

    if s.current == id {
        s.current = 0xFF;
        schedule();
    }
    Ok(())
}

/// Yield the current task's remaining timeslice.
pub fn task_yield() {
    let _cs = CriticalSection::enter();
    let s = sched();
    if s.current != 0xFF {
        let idx = s.current as usize;
        s.tasks[idx].ticks_remaining = TIMESLICE_TICKS;
    }
    schedule();
}

/// Block the current task for `ticks` timer ticks, then re-ready it.
pub fn delay(ticks: u32) {
    let _cs = CriticalSection::enter();
    let s = sched();
    if s.current == 0xFF || !s.started {
        return;
    }
    let idx = s.current as usize;
    s.tasks[idx].delay_ticks = ticks;
    s.tasks[idx].state = TaskState::Blocked;
    schedule();
}

/// Start the scheduler. This does not return — it switches to the
/// highest-priority ready task.
pub fn start() -> ! {
    let s = sched();
    s.started = true;

    let id = s.dequeue_highest().expect("no tasks to run");
    s.current = id;
    s.tasks[id as usize].state = TaskState::Running;
    s.tasks[id as usize].ticks_remaining = TIMESLICE_TICKS;

    // Load the first task's SP and jump into it. We never save the
    // current context because there is no "previous task" yet.
    let sp = s.tasks[id as usize].sp;
    kprintln!("sched: starting '{}' (id={}, prio={})", s.tasks[id as usize].name, id, s.tasks[id as usize].priority);
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

/// Called from the timer tick ISR. Decrements delay timers, wakes blocked
/// tasks, enforces budgets, and triggers preemption when the timeslice expires.
pub fn tick() {
    let s = sched();
    if !s.started || s.current == 0xFF {
        return;
    }

    // Utilization tracking.
    s.total_ticks += 1;
    let idx = s.current as usize;
    s.tasks[idx].total_run_ticks += 1;
    if s.tasks[idx].priority == 255 {
        s.idle_ticks += 1;
    }

    // Budget enforcement.
    if s.tasks[idx].budget_ticks > 0 && s.tasks[idx].budget_remaining > 0 {
        s.tasks[idx].budget_remaining -= 1;
        if s.tasks[idx].budget_remaining == 0 {
            crate::klog_warn!("sched", "task '{}' (id={}) budget exhausted", s.tasks[idx].name, s.tasks[idx].id);
        }
    }

    // Software watchdog.
    crate::watchdog::tick();

    // Check all blocked tasks for delay expiry.
    let mut woke_higher = false;
    let cur_prio = s.tasks[s.current as usize].priority;
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
        schedule();
    }
}

fn schedule() {
    let s = sched();
    if !s.started {
        return;
    }

    let cur = s.current;
    if cur == 0xFF {
        // No current task — just pick the highest.
        if let Some(next_id) = s.dequeue_highest() {
            s.current = next_id;
            s.tasks[next_id as usize].state = TaskState::Running;
            s.tasks[next_id as usize].ticks_remaining = TIMESLICE_TICKS;
        }
        return;
    }

    let cur_idx = cur as usize;
    let cur_prio = s.tasks[cur_idx].priority;
    let cur_runnable = s.tasks[cur_idx].state == TaskState::Running;

    // If the current task is no longer runnable (Blocked, Suspended, etc.),
    // we must switch unconditionally.
    if !cur_runnable {
        if let Some(next_id) = s.dequeue_highest() {
            s.current = next_id;
            s.tasks[next_id as usize].state = TaskState::Running;
            s.tasks[next_id as usize].ticks_remaining = TIMESLICE_TICKS;

            let cur_sp_ptr = &mut s.tasks[cur_idx].sp as *mut u64;
            let next_sp_ptr = &s.tasks[next_id as usize].sp as *const u64;
            unsafe { Aarch64Context::switch(cur_sp_ptr, next_sp_ptr) };
        }
        return;
    }

    // Current task is Running. Check if there's a higher-priority task ready,
    // or a same-priority peer when the timeslice expired.
    let next_prio = s.find_highest_prio();
    let should_switch = match next_prio {
        Some(p) => p < cur_prio as usize || (p == cur_prio as usize && s.tasks[cur_idx].ticks_remaining == 0),
        None => false,
    };

    if !should_switch {
        if s.tasks[cur_idx].ticks_remaining == 0 {
            s.tasks[cur_idx].ticks_remaining = TIMESLICE_TICKS;
        }
        return;
    }

    // Dequeue the next task.
    let next_id = match s.dequeue_highest() {
        Some(id) => id,
        None => return,
    };

    if next_id == cur {
        s.tasks[cur_idx].ticks_remaining = TIMESLICE_TICKS;
        s.tasks[cur_idx].state = TaskState::Running;
        return;
    }

    // Put current task back on the ready queue.
    s.tasks[cur_idx].state = TaskState::Ready;
    s.enqueue(cur);

    // Switch to next.
    let next_idx = next_id as usize;
    s.current = next_id;
    s.tasks[next_idx].state = TaskState::Running;
    s.tasks[next_idx].ticks_remaining = TIMESLICE_TICKS;

    let cur_sp_ptr = &mut s.tasks[cur_idx].sp as *mut u64;
    let next_sp_ptr = &s.tasks[next_idx].sp as *const u64;

    unsafe { Aarch64Context::switch(cur_sp_ptr, next_sp_ptr) };
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

    // Walk the list.
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
    sched().current
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
    let s = sched();
    if s.current != 0xFF {
        s.tasks[s.current as usize].wait_result
    } else {
        WaitResult::Ok
    }
}

pub fn set_task_wait_result(id: u8, result: WaitResult) {
    sched().tasks[id as usize].wait_result = result;
}

/// Block the current task indefinitely (until explicitly woken).
pub fn block_current() {
    let s = sched();
    if s.current == 0xFF || !s.started {
        return;
    }
    let idx = s.current as usize;
    s.tasks[idx].delay_ticks = 0;
    s.tasks[idx].state = TaskState::Blocked;
    schedule();
}

/// Block the current task with a timeout in ticks. 0 = infinite wait.
pub fn block_current_timeout(ticks: u32) {
    let s = sched();
    if s.current == 0xFF || !s.started {
        return;
    }
    let idx = s.current as usize;
    s.tasks[idx].delay_ticks = ticks;
    s.tasks[idx].state = TaskState::Blocked;
    schedule();
}

/// Wake a blocked task and make it ready. Triggers reschedule if appropriate.
pub fn wake_task(id: u8) {
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS || s.tasks[idx].state != TaskState::Blocked {
        return;
    }
    s.tasks[idx].state = TaskState::Ready;
    s.tasks[idx].delay_ticks = 0;
    s.enqueue(id);

    if s.started && s.current != 0xFF {
        let cur_prio = s.tasks[s.current as usize].priority;
        if s.tasks[idx].priority < cur_prio {
            schedule();
        }
    }
}

/// Change a task's effective priority (for PIP/PCP).
pub fn set_priority(id: u8, prio: u8) {
    let s = sched();
    let idx = id as usize;
    if idx >= MAX_TASKS {
        return;
    }
    let old_prio = s.tasks[idx].priority;
    if prio == old_prio {
        return;
    }

    if s.tasks[idx].state == TaskState::Ready {
        remove_from_ready(s, id);
        s.tasks[idx].priority = prio;
        s.enqueue(id);
    } else {
        s.tasks[idx].priority = prio;
    }
}

/// Return info about all non-dormant tasks for the shell.
pub fn task_list() -> [(u8, &'static str, u8, TaskState); MAX_TASKS] {
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
    result
}

pub fn current_task_name() -> &'static str {
    let s = sched();
    if s.current != 0xFF && s.started {
        s.tasks[s.current as usize].name
    } else {
        "none"
    }
}

pub fn task_count() -> u8 {
    sched().task_count
}

// --- Budget and criticality APIs ---

pub fn task_set_budget(id: u8, ticks: u32) {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS && s.tasks[idx].state != TaskState::Dormant {
        s.tasks[idx].budget_ticks = ticks;
        s.tasks[idx].budget_remaining = ticks;
    }
}

pub fn task_get_remaining(id: u8) -> u32 {
    let s = sched();
    s.tasks[id as usize].budget_remaining
}

pub fn task_reset_budget(id: u8) {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS {
        s.tasks[idx].budget_remaining = s.tasks[idx].budget_ticks;
    }
}

pub fn task_set_criticality(id: u8, crit: Criticality) {
    let _cs = CriticalSection::enter();
    let s = sched();
    let idx = id as usize;
    if idx < MAX_TASKS && s.tasks[idx].state != TaskState::Dormant {
        s.tasks[idx].criticality = crit;
    }
}

// --- Utilization ---

pub fn utilization() -> (u64, u64) {
    let s = sched();
    let busy = s.total_ticks - s.idle_ticks;
    (busy, s.total_ticks)
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

/// Return stack info for all non-dormant tasks: (id, name, used_bytes, total_bytes).
pub fn task_stack_info() -> [(u8, &'static str, usize, usize); MAX_TASKS] {
    let _cs = CriticalSection::enter();
    let s = sched();
    let mut result = [(0u8, "", 0usize, 0usize); MAX_TASKS];
    for i in 0..MAX_TASKS {
        if s.tasks[i].state != TaskState::Dormant && s.tasks[i].stack_size > 0 {
            let used = stack_watermark(s.tasks[i].stack_base, s.tasks[i].stack_size);
            result[i] = (s.tasks[i].id, s.tasks[i].name, used, s.tasks[i].stack_size);
        }
    }
    result
}

/// Extended task list for shell display.
pub fn task_list_ext() -> [(u8, &'static str, u8, TaskState, Criticality, u32, u64); MAX_TASKS] {
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
    result
}
