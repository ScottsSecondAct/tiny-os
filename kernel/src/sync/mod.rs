pub mod mutex;
pub mod semaphore;
pub mod events;
pub mod msgqueue;

use crate::os_cfg;
use crate::sched::{self, TaskState};

const MAX_WAITERS: usize = os_cfg::MAX_WAITERS;
const NONE: u8 = 0xFF;

pub struct WaitQueue {
    tasks: [u8; MAX_WAITERS],
    count: u8,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            tasks: [NONE; MAX_WAITERS],
            count: 0,
        }
    }

    pub fn add(&mut self, id: u8) {
        if (self.count as usize) < MAX_WAITERS {
            self.tasks[self.count as usize] = id;
            self.count += 1;
        }
    }

    pub fn remove(&mut self, id: u8) {
        for i in 0..self.count as usize {
            if self.tasks[i] == id {
                let last = self.count as usize - 1;
                self.tasks[i] = self.tasks[last];
                self.tasks[last] = NONE;
                self.count -= 1;
                return;
            }
        }
    }

    /// Pop the highest-priority (lowest numeric value) waiter that is Blocked.
    pub fn pop_highest(&mut self) -> Option<u8> {
        let mut best_idx: Option<usize> = None;
        let mut best_prio: u8 = 255;
        for i in 0..self.count as usize {
            let id = self.tasks[i];
            if sched::get_state(id) == TaskState::Blocked {
                let prio = sched::get_priority(id);
                if best_idx.is_none() || prio < best_prio {
                    best_prio = prio;
                    best_idx = Some(i);
                }
            }
        }
        if let Some(idx) = best_idx {
            let id = self.tasks[idx];
            let last = self.count as usize - 1;
            self.tasks[idx] = self.tasks[last];
            self.tasks[last] = NONE;
            self.count -= 1;
            Some(id)
        } else {
            None
        }
    }

    /// Highest priority among blocked waiters.
    pub fn highest_blocked_priority(&self) -> Option<u8> {
        let mut best: Option<u8> = None;
        for i in 0..self.count as usize {
            let id = self.tasks[i];
            if sched::get_state(id) == TaskState::Blocked {
                let prio = sched::get_priority(id);
                best = Some(match best {
                    Some(p) if prio < p => prio,
                    Some(p) => p,
                    None => prio,
                });
            }
        }
        best
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Remove any entries for tasks no longer in Blocked state (lazy cleanup).
    pub fn cleanup_stale(&mut self) {
        let mut i = 0;
        while i < self.count as usize {
            let id = self.tasks[i];
            if sched::get_state(id) != TaskState::Blocked {
                let last = self.count as usize - 1;
                self.tasks[i] = self.tasks[last];
                self.tasks[last] = NONE;
                self.count -= 1;
            } else {
                i += 1;
            }
        }
    }
}
