// Centralized OS configuration constants with compile-time validation.
//
// All system-wide tuning lives here. Module-local sizing constants
// (ARP cache, socket tables, etc.) remain in their own modules.

// --- Feature-derived booleans ---

pub const SAFETY_CRITICAL: bool = cfg!(feature = "safety-critical");

// --- Task / Scheduler ---

pub const MAX_TASKS: usize = 32;
pub const PRIO_LEVELS: usize = 256;
pub const PRIO_BITMAP_WORDS: usize = PRIO_LEVELS / 64;
pub const TIMESLICE_EN: bool = true;
pub const TIMESLICE_TICKS: u32 = 10;
pub const IDLE_STACK_SIZE: usize = 4096;

// --- SMP ---

pub const SMP_CORES: usize = 4;

// --- Tick / Timer ---

pub const TICK_RATE_HZ: u32 = 1000;

// --- Stacks ---

pub const USER_STACK_SIZE: usize = 16384;
pub const KERN_STACK_SIZE: usize = 8192;

// --- Synchronization ---

pub const MAX_MUTEX_NEST: u8 = 8;
pub const MAX_WAITERS: usize = 32;
pub const MAX_EVENT_WAITERS: usize = 16;

// --- Watchdog ---

pub const WDT_TIMEOUT_MS: u32 = 5000;
pub const WDT_KICK_INTERVAL_MS: u32 = 2000;

// --- Health Monitor ---

pub const HEALTH_CHECK_INTERVAL_MS: u32 = 5000;
pub const STACK_WARN_PERCENT: usize = 80;

// --- Memory ---

pub const HEAP_PAGES: usize = 64; // 256 KB initial heap
pub const DMA_POOL_BASE: usize = 0x0100_0000;
pub const DMA_POOL_SIZE: usize = 0x0020_0000; // 2 MB
pub const MIN_DRAM_MB: usize = 64;
pub const MAX_POOLS: usize = 16;

// --- Budget / Deadline ---

pub const BUDGET_EN: bool = if SAFETY_CRITICAL { true } else { false };
pub const DEADLINE_DETECT_EN: bool = false;

// --- Interrupt ---

pub const MAX_ISR_ENTRIES: usize = 256;
pub const MAX_ISR_PRIO: u8 = 0x40;

// --- Logging ---

pub const LOG_BUFFER_SIZE: usize = 64;
pub const LOG_MSG_SIZE: usize = 80;
pub const LOG_MODULE_SIZE: usize = 8;

// --- Diagnostics ---

pub const STATS_EN: bool = if SAFETY_CRITICAL { true } else { false };
pub const DIAG_REGION_SIZE: usize = 4096;
pub const REBOOT_ON_FAULT: bool = false;

// --- ELF Loader ---

pub const LOADER_KERN_STACK_SIZE: usize = 8192;
pub const LOADER_USER_STACK_PAGES: usize = 4;
pub const LOADER_MAX_SEGMENTS: usize = 8;

// --- Syscall ---

pub const MAX_USER_STR: usize = 256;
pub const MAX_USER_BUF: usize = 4096;

// --- Driver Registry ---

pub const MAX_DRIVERS: usize = 16;

// --- Block Cache ---

pub const CACHE_LINES: usize = 32;

// --- Filesystem ---

pub const MAX_OPEN_FILES: usize = 16;

// --- Network ---

pub const MAX_SOCKETS: usize = 8;
pub const MAX_TCP_CONNS: usize = 4;
pub const MAX_ARP_ENTRIES: usize = 16;

// --- Netbuf ---

pub const NETBUF_SLOT_SIZE: usize = 2048;
pub const NETBUF_DATA_CAPACITY: usize = 1536;
pub const NETBUF_HEADROOM: usize = 64;

// =========================================================================
// Compile-time validation (spec section 10.2, 14 assertions)
// =========================================================================

const _: () = assert!(MAX_TASKS >= 1 && MAX_TASKS <= 256,
    "OS_CFG_MAX_TASKS must be 1..=256");
const _: () = assert!(TICK_RATE_HZ >= 100 && TICK_RATE_HZ <= 10_000,
    "OS_CFG_TICK_RATE_HZ must be 100..=10000");
const _: () = assert!(PRIO_LEVELS == 8 || PRIO_LEVELS == 32 || PRIO_LEVELS == 256,
    "OS_CFG_PRIO_LEVELS must be 8, 32, or 256");
const _: () = assert!(TIMESLICE_TICKS >= 1 && TIMESLICE_TICKS <= 1000,
    "OS_CFG_TIMESLICE_TICKS must be 1..=1000");
const _: () = assert!(SMP_CORES >= 1 && SMP_CORES <= 4,
    "OS_CFG_SMP_CORES must be 1..=4");
const _: () = assert!(USER_STACK_SIZE >= 1024 && USER_STACK_SIZE <= 1_048_576,
    "OS_CFG_USER_STACK_SIZE must be 1024..=1048576");
const _: () = assert!(KERN_STACK_SIZE >= 1024 && KERN_STACK_SIZE <= 65_536,
    "OS_CFG_KERN_STACK_SIZE must be 1024..=65536");
const _: () = assert!(MAX_MUTEX_NEST >= 1 && MAX_MUTEX_NEST <= 32,
    "OS_CFG_MAX_MUTEX_NEST must be 1..=32");
const _: () = assert!(HEALTH_CHECK_INTERVAL_MS >= 10 && HEALTH_CHECK_INTERVAL_MS <= 60_000,
    "OS_CFG_HEALTH_CHECK_INTERVAL must be 10..=60000");
const _: () = assert!(WDT_TIMEOUT_MS >= 100 && WDT_TIMEOUT_MS <= 30_000,
    "OS_CFG_WDT_TIMEOUT_MS must be 100..=30000");
const _: () = assert!(DIAG_REGION_SIZE >= 256 && DIAG_REGION_SIZE <= 65_536,
    "OS_CFG_DIAG_REGION_SIZE must be 256..=65536");
const _: () = assert!(MAX_POOLS >= 1 && MAX_POOLS <= 64,
    "OS_CFG_MAX_POOLS must be 1..=64");
const _: () = assert!(MIN_DRAM_MB >= 16 && MIN_DRAM_MB <= 16_384,
    "OS_CFG_MIN_DRAM_MB must be 16..=16384");

// Safety-critical mode enforcement: budget monitoring must be enabled.
const _: () = assert!(!SAFETY_CRITICAL || BUDGET_EN,
    "OS_CFG_SAFETY_CRITICAL requires BUDGET_EN = true");
