use core::cell::UnsafeCell;
use crate::os_cfg;
use crate::sched::CriticalSection;

pub trait Driver: Sync {
    fn name(&self) -> &'static str;
    fn init(&self) -> Result<(), &'static str>;
    fn status(&self) -> &'static str;
}

const MAX_DRIVERS: usize = os_cfg::MAX_DRIVERS;

struct Registry {
    drivers: [Option<&'static dyn Driver>; MAX_DRIVERS],
    count: usize,
}

impl Registry {
    const fn new() -> Self {
        Self {
            drivers: [None; MAX_DRIVERS],
            count: 0,
        }
    }
}

struct RegistryCell(UnsafeCell<Registry>);
// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for RegistryCell {}

static REG: RegistryCell = RegistryCell(UnsafeCell::new(Registry::new()));

fn registry() -> &'static mut Registry {
    // SAFETY: Caller holds CriticalSection.
    unsafe { &mut *REG.0.get() }
}

pub fn register(driver: &'static dyn Driver) -> Result<usize, &'static str> {
    let _cs = CriticalSection::enter();
    let r = registry();
    if r.count >= MAX_DRIVERS {
        return Err("driver registry full");
    }
    let id = r.count;
    r.drivers[id] = Some(driver);
    r.count += 1;
    Ok(id)
}

pub fn count() -> usize {
    let _cs = CriticalSection::enter();
    registry().count
}

pub fn list() -> [Option<(&'static str, &'static str)>; MAX_DRIVERS] {
    let _cs = CriticalSection::enter();
    let r = registry();
    let mut result = [None; MAX_DRIVERS];
    for i in 0..r.count {
        if let Some(d) = r.drivers[i] {
            result[i] = Some((d.name(), d.status()));
        }
    }
    result
}
