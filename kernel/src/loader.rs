use crate::sched::Criticality;
use crate::{fs, kprintln, mm, sched};
use arch::aarch64::mmu::{self, UserMapping, PAGE_SIZE_4K};

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EM_AARCH64: u16 = 183;
const ET_DYN: u16 = 3;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const R_AARCH64_RELATIVE: u32 = 1027;

use crate::os_cfg;

const USER_STACK_PAGES: usize = os_cfg::LOADER_USER_STACK_PAGES;
const KERNEL_STACK_SIZE: usize = os_cfg::LOADER_KERN_STACK_SIZE;
const MAX_SEGMENTS: usize = os_cfg::LOADER_MAX_SEGMENTS;

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Dyn {
    d_tag: i64,
    d_val: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Rela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

#[derive(Debug)]
pub enum LoadError {
    FsError,
    NotElf,
    WrongArch,
    NotPie,
    TooManySegments,
    OutOfMemory,
    BadPhdr,
}

struct LoadedSegment {
    phys_base: usize,
    vaddr: u64,
    memsz: usize,
    pages: usize,
    executable: bool,
}

fn read_exact(fd: usize, buf: &mut [u8]) -> Result<(), LoadError> {
    let mut pos = 0;
    while pos < buf.len() {
        let n = fs::read(fd, &mut buf[pos..]).map_err(|_| LoadError::FsError)?;
        if n == 0 {
            return Err(LoadError::FsError);
        }
        pos += n;
    }
    Ok(())
}

fn skip_bytes(fd: usize, count: usize) -> Result<(), LoadError> {
    let mut remaining = count;
    let mut skip_buf = [0u8; 512];
    while remaining > 0 {
        let chunk = remaining.min(512);
        let n = fs::read(fd, &mut skip_buf[..chunk]).map_err(|_| LoadError::FsError)?;
        if n == 0 {
            return Err(LoadError::FsError);
        }
        remaining -= n;
    }
    Ok(())
}

fn reopen_at(path: &str, offset: usize) -> Result<usize, LoadError> {
    let fd = fs::open(path, false).map_err(|_| LoadError::FsError)?;
    if offset > 0 {
        skip_bytes(fd, offset)?;
    }
    Ok(fd)
}

pub fn load_and_exec(path: &str) -> Result<u8, LoadError> {
    // Read the ELF header.
    let fd = fs::open(path, false).map_err(|_| LoadError::FsError)?;
    let mut ehdr_buf = [0u8; core::mem::size_of::<Elf64Ehdr>()];
    read_exact(fd, &mut ehdr_buf)?;

    let ehdr: Elf64Ehdr = unsafe { core::ptr::read_unaligned(ehdr_buf.as_ptr().cast()) };

    if ehdr.e_ident[0..4] != ELF_MAGIC {
        return Err(LoadError::NotElf);
    }
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(LoadError::WrongArch);
    }
    if ehdr.e_ident[5] != ELFDATA2LSB {
        return Err(LoadError::WrongArch);
    }
    if ehdr.e_machine != EM_AARCH64 {
        return Err(LoadError::WrongArch);
    }
    if ehdr.e_type != ET_DYN {
        return Err(LoadError::NotPie);
    }

    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    if phnum > MAX_SEGMENTS * 2 {
        fs::close(fd).ok();
        return Err(LoadError::TooManySegments);
    }

    // Read program headers. They may not immediately follow the ELF header.
    let already_read = core::mem::size_of::<Elf64Ehdr>();
    if phoff > already_read {
        skip_bytes(fd, phoff - already_read)?;
    }
    let phdrs_size = phnum * phentsize;
    let mut phdr_buf = [0u8; 8 * core::mem::size_of::<Elf64Phdr>()];
    if phdrs_size > phdr_buf.len() {
        fs::close(fd).ok();
        return Err(LoadError::TooManySegments);
    }
    read_exact(fd, &mut phdr_buf[..phdrs_size])?;
    fs::close(fd).ok();

    // Parse program headers — collect PT_LOAD segments and PT_DYNAMIC.
    let mut loads: [(Elf64Phdr, bool); MAX_SEGMENTS] =
        [(unsafe { core::mem::zeroed() }, false); MAX_SEGMENTS];
    let mut num_loads = 0usize;
    let mut dyn_offset = 0u64;
    let mut dyn_size = 0u64;

    for i in 0..phnum {
        let off = i * phentsize;
        let phdr: Elf64Phdr = unsafe { core::ptr::read_unaligned(phdr_buf[off..].as_ptr().cast()) };
        match phdr.p_type {
            PT_LOAD => {
                if num_loads >= MAX_SEGMENTS {
                    return Err(LoadError::TooManySegments);
                }
                loads[num_loads] = (phdr, true);
                num_loads += 1;
            }
            PT_DYNAMIC => {
                dyn_offset = phdr.p_offset;
                dyn_size = phdr.p_filesz;
            }
            _ => {}
        }
    }

    if num_loads == 0 {
        return Err(LoadError::BadPhdr);
    }

    // Calculate total memory extent (vaddr range across all LOAD segments).
    let vaddr_min = loads[..num_loads]
        .iter()
        .map(|(p, _)| p.p_vaddr)
        .min()
        .unwrap();
    let vaddr_max = loads[..num_loads]
        .iter()
        .map(|(p, _)| p.p_vaddr + p.p_memsz)
        .max()
        .unwrap();

    let total_size = (vaddr_max - vaddr_min) as usize;
    let total_pages = (total_size + PAGE_SIZE_4K - 1) / PAGE_SIZE_4K;

    // Allocate contiguous physical pages for the binary.
    let phys_base = mm::alloc_pages(total_pages).ok_or(LoadError::OutOfMemory)?;

    // Zero the entire region.
    unsafe {
        core::ptr::write_bytes(phys_base as *mut u8, 0, total_pages * PAGE_SIZE_4K);
    }

    // Load base: physical address minus the minimum vaddr so that
    // segment vaddr + load_base = physical address.
    let load_base = phys_base as i64 - vaddr_min as i64;

    // Load each PT_LOAD segment from the file.
    let mut segments: [LoadedSegment; MAX_SEGMENTS] = unsafe { core::mem::zeroed() };
    let mut num_segs = 0usize;

    for i in 0..num_loads {
        let phdr = loads[i].0;
        let dest_addr = (phdr.p_vaddr as i64 + load_base) as usize;
        let filesz = phdr.p_filesz as usize;

        if filesz > 0 {
            let fd = reopen_at(path, phdr.p_offset as usize)?;
            let dest_slice =
                unsafe { core::slice::from_raw_parts_mut(dest_addr as *mut u8, filesz) };
            read_exact(fd, dest_slice)?;
            fs::close(fd).ok();
        }

        // Record this as a mapped segment.
        let seg_start = dest_addr & !(PAGE_SIZE_4K - 1);
        let seg_end = (dest_addr + phdr.p_memsz as usize + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
        let seg_pages = (seg_end - seg_start) / PAGE_SIZE_4K;

        segments[num_segs] = LoadedSegment {
            phys_base: seg_start,
            vaddr: phdr.p_vaddr,
            memsz: phdr.p_memsz as usize,
            pages: seg_pages,
            executable: (phdr.p_flags & PF_X) != 0,
        };
        num_segs += 1;
    }

    // Apply R_AARCH64_RELATIVE relocations from the dynamic section.
    if dyn_size > 0 {
        let fd = reopen_at(path, dyn_offset as usize)?;
        let dyn_count = dyn_size as usize / core::mem::size_of::<Elf64Dyn>();
        let mut rela_off = 0u64;
        let mut rela_sz = 0u64;

        for _ in 0..dyn_count {
            let mut dyn_buf = [0u8; core::mem::size_of::<Elf64Dyn>()];
            read_exact(fd, &mut dyn_buf)?;
            let dyn_entry: Elf64Dyn = unsafe { core::ptr::read_unaligned(dyn_buf.as_ptr().cast()) };
            match dyn_entry.d_tag {
                DT_RELA => rela_off = dyn_entry.d_val,
                DT_RELASZ => rela_sz = dyn_entry.d_val,
                _ => {}
            }
        }
        fs::close(fd).ok();

        if rela_sz > 0 {
            let fd = reopen_at(path, rela_off as usize)?;
            let rela_count = rela_sz as usize / core::mem::size_of::<Elf64Rela>();

            for _ in 0..rela_count {
                let mut rela_buf = [0u8; core::mem::size_of::<Elf64Rela>()];
                read_exact(fd, &mut rela_buf)?;
                let rela: Elf64Rela =
                    unsafe { core::ptr::read_unaligned(rela_buf.as_ptr().cast()) };

                let r_type = (rela.r_info & 0xFFFF_FFFF) as u32;
                if r_type == R_AARCH64_RELATIVE {
                    let target = (rela.r_offset as i64 + load_base) as usize;
                    let value = (rela.r_addend + load_base) as u64;
                    unsafe {
                        core::ptr::write_unaligned(target as *mut u64, value);
                    }
                }
            }
            fs::close(fd).ok();
        }
    }

    // Allocate a user stack.
    let stack_phys = mm::alloc_pages(USER_STACK_PAGES).ok_or(LoadError::OutOfMemory)?;
    unsafe {
        core::ptr::write_bytes(stack_phys as *mut u8, 0, USER_STACK_PAGES * PAGE_SIZE_4K);
    }
    let stack_top = stack_phys + USER_STACK_PAGES * PAGE_SIZE_4K;

    // Build UserMapping array: one per segment + stack.
    let mut mappings: [UserMapping; MAX_SEGMENTS + 1] = unsafe { core::mem::zeroed() };
    let mut map_count = 0;

    // Merge overlapping/adjacent segments with same permissions, or just map each.
    for i in 0..num_segs {
        mappings[map_count] = UserMapping {
            base: segments[i].phys_base,
            pages: segments[i].pages,
            executable: segments[i].executable,
        };
        map_count += 1;
    }

    // Stack mapping (last, so guard page is placed below it).
    mappings[map_count] = UserMapping {
        base: stack_phys,
        pages: USER_STACK_PAGES,
        executable: false,
    };
    map_count += 1;

    // Create page tables.
    let ttbr0 = unsafe { mmu::create_user_page_table_mapped(&mappings[..map_count]) };

    // Allocate a kernel stack for this task.
    let kernel_stack_phys =
        mm::alloc_pages(KERNEL_STACK_SIZE / PAGE_SIZE_4K).ok_or(LoadError::OutOfMemory)?;
    let kernel_stack: &'static mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(kernel_stack_phys as *mut u8, KERNEL_STACK_SIZE) };

    let entry = (ehdr.e_entry as i64 + load_base) as usize;

    let task_id = sched::task_create_user(
        "elf-app",
        100,
        Criticality::Standard,
        kernel_stack,
        entry,
        stack_top,
        0,
        ttbr0,
    )
    .map_err(|_| LoadError::OutOfMemory)?;

    kprintln!(
        "loader: loaded '{}' at {:#x}, entry {:#x}, task {}",
        path,
        phys_base,
        entry,
        task_id
    );

    Ok(task_id)
}
