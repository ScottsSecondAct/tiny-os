use crate::smp::SmpBoot;

pub const MAX_CORES: usize = 4;

extern "C" {
    static SMP_RELEASE_TABLE: [u64; MAX_CORES];
    fn secondary_boot();
}

pub struct Aarch64Smp;

impl SmpBoot for Aarch64Smp {
    fn core_id() -> usize {
        let mpidr: u64;
        unsafe { core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr) };
        (mpidr & 0xFF) as usize
    }

    fn num_cores() -> usize {
        MAX_CORES
    }

    fn start_core(core_id: usize, entry: extern "C" fn(usize) -> !) {
        assert!(core_id > 0 && core_id < MAX_CORES, "invalid secondary core id");

        // Write the secondary_boot entry point to the spin-table slot.
        // secondary_boot handles EL drop, per-core stack, then calls secondary_main.
        // We store secondary_boot's address; the Rust entry will be called from there.
        let _ = entry; // entry is registered via set_secondary_entry() instead
        let release = unsafe { &SMP_RELEASE_TABLE as *const _ as *mut u64 };
        let addr = secondary_boot as *const () as u64;
        unsafe {
            core::ptr::write_volatile(release.add(core_id), addr);
        }

        // Data synchronization barrier + SEV to wake the spinning core.
        unsafe {
            core::arch::asm!("dsb sy");
            core::arch::asm!("sev");
        }
    }
}

/// Read the current core ID from MPIDR_EL1.
#[inline(always)]
pub fn core_id() -> usize {
    Aarch64Smp::core_id()
}

/// Wake a secondary core. It will execute the EL drop sequence in
/// secondary_boot (assembly), then call `secondary_main(core_id)` in Rust.
pub fn start_core(core_id: usize) {
    assert!(core_id > 0 && core_id < MAX_CORES, "invalid secondary core id");

    let release = unsafe { &SMP_RELEASE_TABLE as *const _ as *mut u64 };
    let addr = secondary_boot as *const () as u64;
    unsafe {
        core::ptr::write_volatile(release.add(core_id), addr);
        core::arch::asm!("dsb sy");
        core::arch::asm!("sev");
    }
}
