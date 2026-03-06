use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Best-effort: print the panic message. If the UART writer was never
    // initialized (very early panic), this is silently dropped.
    crate::kprintln!();
    crate::kprintln!("*** KERNEL PANIC ***");

    if let Some(location) = info.location() {
        crate::kprintln!("  at {}:{}", location.file(), location.line());
    }

    crate::kprintln!("  {}", info.message());

    // Halt: spin in WFE so the core idles rather than burning power.
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}
