use crate::os_cfg;

static mut PAC_ACTIVE: bool = false;
static mut PAC_SUPPORTED: bool = false;

pub fn detect() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        let isar1: u64;
        unsafe { core::arch::asm!("mrs {}, ID_AA64ISAR1_EL1", out(reg) isar1) };
        let api = (isar1 >> 8) & 0xF;
        let apa = (isar1 >> 4) & 0xF;
        api >= 1 || apa >= 1
    }
    #[cfg(not(target_arch = "aarch64"))]
    false
}

pub fn init() {
    if !os_cfg::PAC_EN {
        return;
    }

    let supported = detect();
    unsafe { PAC_SUPPORTED = supported; }

    if !supported {
        crate::kprintln!("pac: not supported on this CPU");
        return;
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        // Load PAC instruction A keys (APIAKey).
        // S3_0_C2_C1_0 = APIAKeyLo_EL1, S3_0_C2_C1_1 = APIAKeyHi_EL1.
        // In production these would come from a hardware RNG.
        core::arch::asm!(
            "msr S3_0_C2_C1_0, {lo}",
            "msr S3_0_C2_C1_1, {hi}",
            lo = in(reg) 0x4F6E_5469_6E79_4F53u64,
            hi = in(reg) 0x5041_4352_5453_4B31u64,
        );

        // Enable pointer authentication in SCTLR_EL1 (EnIA, bit 31).
        let mut sctlr: u64;
        core::arch::asm!("mrs {}, SCTLR_EL1", out(reg) sctlr);
        sctlr |= 1 << 31; // EnIA — enable PACIA/AUTIA instructions
        core::arch::asm!("msr SCTLR_EL1, {}", "isb", in(reg) sctlr);

        PAC_ACTIVE = true;
    }

    crate::kprintln!("pac: ARMv8.3 pointer authentication enabled");
}

pub fn is_active() -> bool {
    unsafe { PAC_ACTIVE }
}

pub fn is_supported() -> bool {
    unsafe { PAC_SUPPORTED }
}

pub fn status() {
    crate::kprintln!("pac: supported={}, active={}, cfg={}",
        is_supported(), is_active(), os_cfg::PAC_EN);
    if is_active() {
        crate::kprintln!("pac: PACIASP/AUTIASP protecting return addresses");
    }
}
