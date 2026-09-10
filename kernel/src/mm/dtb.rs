// Minimal Flattened Device Tree (FDT) parser.
//
// Extracts the /memory node's `reg` property to discover RAM base and size.
// Falls back to BSP defaults if the DTB is absent or unparseable.

const FDT_MAGIC: u32 = 0xD00DFEED;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_END: u32 = 9;

#[no_mangle]
static mut DTB_PTR: u64 = 0;

pub fn dtb_ptr() -> usize {
    // SAFETY: Written once by boot.S before kmain.
    unsafe { DTB_PTR as usize }
}

pub struct RamRegion {
    pub base: u64,
    pub size: u64,
}

pub fn find_memory() -> Option<RamRegion> {
    let ptr = dtb_ptr();
    if ptr == 0 {
        return None;
    }

    // SAFETY: DTB pointer was passed by firmware/QEMU and is valid memory.
    let header = unsafe { &*(ptr as *const FdtHeader) };
    if u32::from_be(header.magic) != FDT_MAGIC {
        return None;
    }

    let total_size = u32::from_be(header.totalsize) as usize;
    if total_size < core::mem::size_of::<FdtHeader>() {
        return None;
    }

    let struct_off = u32::from_be(header.off_dt_struct) as usize;
    let strings_off = u32::from_be(header.off_dt_strings) as usize;

    let base = ptr as *const u8;
    let struct_base = unsafe { base.add(struct_off) };
    let strings_base = unsafe { base.add(strings_off) };

    parse_struct(struct_base, strings_base, total_size - struct_off)
}

#[repr(C)]
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

fn parse_struct(base: *const u8, strings: *const u8, max_len: usize) -> Option<RamRegion> {
    let mut off: usize = 0;
    let mut depth: i32 = 0;
    let mut in_memory = false;

    loop {
        if off + 4 > max_len {
            return None;
        }
        let token = read_be32(base, off);
        off += 4;

        match token {
            FDT_BEGIN_NODE => {
                let name = read_cstr(base, off);
                off += align4(name.len() + 1);
                depth += 1;
                // /memory or /memory@... at depth 1
                in_memory = depth == 1
                    && (name == "memory" || name.starts_with("memory@"));
            }
            FDT_END_NODE => {
                if in_memory && depth == 1 {
                    in_memory = false;
                }
                depth -= 1;
            }
            FDT_PROP => {
                if off + 8 > max_len {
                    return None;
                }
                let val_len = read_be32(base, off) as usize;
                let name_off = read_be32(base, off + 4) as usize;
                off += 8;

                if in_memory && depth == 1 {
                    let prop_name = read_cstr(strings, name_off);
                    if prop_name == "reg" && val_len >= 16 {
                        let prop_base = unsafe { base.add(off) };
                        let ram_base = read_be64_ptr(prop_base, 0);
                        let ram_size = read_be64_ptr(prop_base, 8);
                        return Some(RamRegion {
                            base: ram_base,
                            size: ram_size,
                        });
                    }
                }
                off += align4(val_len);
            }
            FDT_END => return None,
            _ => {} // FDT_NOP or unknown
        }
    }
}

fn read_be32(base: *const u8, off: usize) -> u32 {
    // SAFETY: Caller ensures off is within bounds.
    unsafe {
        let p = base.add(off) as *const u32;
        u32::from_be(core::ptr::read_unaligned(p))
    }
}

fn read_be64_ptr(base: *const u8, off: usize) -> u64 {
    unsafe {
        let p = base.add(off) as *const u64;
        u64::from_be(core::ptr::read_unaligned(p))
    }
}

fn read_cstr(base: *const u8, off: usize) -> &'static str {
    unsafe {
        let start = base.add(off);
        let mut len = 0usize;
        while *start.add(len) != 0 && len < 256 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(start, len))
    }
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}
