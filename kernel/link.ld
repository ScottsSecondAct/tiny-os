/* tiny_os linker script for AArch64 bare-metal (Raspberry Pi 5 / QEMU raspi3b).
 *
 * The RPi firmware loads the kernel binary to physical address 0x80000 and
 * jumps to it. We place .text.boot first so that `_start` lands exactly there.
 */

ENTRY(_start)

SECTIONS
{
    . = 0x80000;

    /* ------------------------------------------------------------------ */
    /* .text.boot: must be first — this is where _start lives.            */
    /* KEEP prevents the linker from discarding it as unreferenced.       */
    /* ------------------------------------------------------------------ */
    .text.boot : {
        KEEP(*(.text.boot))
    }

    /* ------------------------------------------------------------------ */
    /* .text: all other code.                                              */
    /* ------------------------------------------------------------------ */
    .text : {
        *(.text .text.*)
    }

    /* ------------------------------------------------------------------ */
    /* .rodata: read-only data; page-aligned so we can map it RO later.   */
    /* ------------------------------------------------------------------ */
    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
    }

    /* ------------------------------------------------------------------ */
    /* .data: initialized read-write data; page-aligned.                  */
    /* ------------------------------------------------------------------ */
    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
    }

    /* ------------------------------------------------------------------ */
    /* .bss: zero-initialized data.                                        */
    /*   16-byte aligned so boot.S can zero it with `stp xzr, xzr`.      */
    /* ------------------------------------------------------------------ */
    . = ALIGN(16);
    __bss_start = .;
    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }
    . = ALIGN(16);
    __bss_end = .;

    /* ------------------------------------------------------------------ */
    /* Stack: 512 KiB, placed immediately after BSS.                      */
    /*   __stack_bottom is the low address (grows up is the wrong way to  */
    /*   think about it; AArch64 stacks grow down toward __stack_bottom). */
    /*   __stack_top is the initial SP value (16-byte aligned per ABI).   */
    /* ------------------------------------------------------------------ */
    . = ALIGN(16);
    __stack_bottom = .;
    . += 512K;
    . = ALIGN(16);
    __stack_top = .;

    /* ------------------------------------------------------------------ */
    /* Discard unwanted sections that inflate the binary.                 */
    /* ------------------------------------------------------------------ */
    /DISCARD/ : {
        *(.eh_frame)
        *(.note.gnu.build-id)
        *(.ARM.exidx*)
        *(.comment)
    }
}
