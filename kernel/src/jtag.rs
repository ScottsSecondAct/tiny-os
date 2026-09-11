use crate::{kprintln, os_cfg};

pub fn lockdown() {
    if !os_cfg::DEBUG_LOCKDOWN {
        return;
    }

    #[cfg(feature = "bsp-rpi5")]
    {
        // Disable JTAG by reconfiguring GPIO 22-27 (default JTAG pins) to input mode
        // with pull-down, rendering the debug port inert.
        use crate::periph;
        use arch::gpio::{PinMode, PullMode};
        for pin in 22..=27 {
            let _ = periph::gpio_set_mode(pin, PinMode::Input);
            let _ = periph::gpio_set_pull(pin, PullMode::Down);
        }
        kprintln!("jtag: debug port locked down (GPIO 22-27 disabled)");
    }

    #[cfg(feature = "bsp-qemu")]
    {
        kprintln!("jtag: debug lockdown not applicable (QEMU)");
    }

    // Disable OS lock debug access via OSLAR_EL1 (locks debug registers).
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("msr oslar_el1, {}", in(reg) 1u64);
        core::arch::asm!("isb");
    }
}
