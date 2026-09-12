use super::WaitQueue;
use crate::sched::{self, CriticalSection, WaitResult};
use core::cell::UnsafeCell;

struct MsgQueueInner<const MSG_SIZE: usize, const CAPACITY: usize> {
    buffer: [[u8; MSG_SIZE]; CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    send_waiters: WaitQueue,
    recv_waiters: WaitQueue,
}

pub struct MsgQueue<const MSG_SIZE: usize, const CAPACITY: usize> {
    inner: UnsafeCell<MsgQueueInner<MSG_SIZE, CAPACITY>>,
}

// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl<const M: usize, const C: usize> Sync for MsgQueue<M, C> {}

impl<const MSG_SIZE: usize, const CAPACITY: usize> MsgQueue<MSG_SIZE, CAPACITY> {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(MsgQueueInner {
                buffer: [[0; MSG_SIZE]; CAPACITY],
                head: 0,
                tail: 0,
                count: 0,
                send_waiters: WaitQueue::new(),
                recv_waiters: WaitQueue::new(),
            }),
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn inner(&self) -> &mut MsgQueueInner<MSG_SIZE, CAPACITY> {
        // SAFETY: Caller holds CriticalSection.
        unsafe { &mut *self.inner.get() }
    }

    /// Send a message. Blocks if the queue is full.
    pub fn send(&self, msg: &[u8; MSG_SIZE]) -> Result<(), &'static str> {
        self.send_impl(msg, 0)
    }

    pub fn send_timeout(&self, msg: &[u8; MSG_SIZE], ticks: u32) -> Result<(), &'static str> {
        self.send_impl(msg, ticks)
    }

    fn send_impl(&self, msg: &[u8; MSG_SIZE], timeout: u32) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let q = self.inner();

        // If there's a receiver waiting, hand the message directly.
        q.recv_waiters.cleanup_stale();
        if let Some(waiter) = q.recv_waiters.pop_highest() {
            // Copy message into the queue slot that the receiver will read.
            q.buffer[q.tail] = *msg;
            q.tail = (q.tail + 1) % CAPACITY;
            q.count += 1;
            sched::set_task_wait_result(waiter, WaitResult::Ok);
            sched::wake_task(waiter);
            return Ok(());
        }

        if q.count < CAPACITY {
            q.buffer[q.tail] = *msg;
            q.tail = (q.tail + 1) % CAPACITY;
            q.count += 1;
            return Ok(());
        }

        // Queue full — block.
        let me = sched::current_id();
        q.send_waiters.add(me);
        sched::set_task_wait_result(me, WaitResult::Timeout);

        if timeout > 0 {
            sched::block_current_timeout(timeout);
        } else {
            sched::block_current();
        }

        if sched::get_wait_result() == WaitResult::Timeout {
            q.send_waiters.remove(me);
            return Err("msgqueue: send timeout");
        }

        // Woken because space is available — enqueue now.
        let q2 = self.inner();
        q2.buffer[q2.tail] = *msg;
        q2.tail = (q2.tail + 1) % CAPACITY;
        q2.count += 1;
        Ok(())
    }

    /// Receive a message. Blocks if the queue is empty.
    pub fn recv(&self, buf: &mut [u8; MSG_SIZE]) -> Result<(), &'static str> {
        self.recv_impl(buf, 0)
    }

    pub fn recv_timeout(&self, buf: &mut [u8; MSG_SIZE], ticks: u32) -> Result<(), &'static str> {
        self.recv_impl(buf, ticks)
    }

    fn recv_impl(&self, buf: &mut [u8; MSG_SIZE], timeout: u32) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let q = self.inner();

        if q.count > 0 {
            *buf = q.buffer[q.head];
            q.head = (q.head + 1) % CAPACITY;
            q.count -= 1;

            // Wake a blocked sender if any.
            q.send_waiters.cleanup_stale();
            if let Some(waiter) = q.send_waiters.pop_highest() {
                sched::set_task_wait_result(waiter, WaitResult::Ok);
                sched::wake_task(waiter);
            }
            return Ok(());
        }

        // Queue empty — block.
        let me = sched::current_id();
        q.recv_waiters.add(me);
        sched::set_task_wait_result(me, WaitResult::Timeout);

        if timeout > 0 {
            sched::block_current_timeout(timeout);
        } else {
            sched::block_current();
        }

        if sched::get_wait_result() == WaitResult::Timeout {
            q.recv_waiters.remove(me);
            return Err("msgqueue: recv timeout");
        }

        // Woken because a message arrived — dequeue it.
        let q2 = self.inner();
        *buf = q2.buffer[q2.head];
        q2.head = (q2.head + 1) % CAPACITY;
        q2.count -= 1;
        Ok(())
    }

    pub fn try_send(&self, msg: &[u8; MSG_SIZE]) -> bool {
        let _cs = CriticalSection::enter();
        let q = self.inner();
        if q.count >= CAPACITY {
            return false;
        }
        q.buffer[q.tail] = *msg;
        q.tail = (q.tail + 1) % CAPACITY;
        q.count += 1;

        q.recv_waiters.cleanup_stale();
        if let Some(waiter) = q.recv_waiters.pop_highest() {
            sched::set_task_wait_result(waiter, WaitResult::Ok);
            sched::wake_task(waiter);
        }
        true
    }

    pub fn try_recv(&self, buf: &mut [u8; MSG_SIZE]) -> bool {
        let _cs = CriticalSection::enter();
        let q = self.inner();
        if q.count == 0 {
            return false;
        }
        *buf = q.buffer[q.head];
        q.head = (q.head + 1) % CAPACITY;
        q.count -= 1;

        q.send_waiters.cleanup_stale();
        if let Some(waiter) = q.send_waiters.pop_highest() {
            sched::set_task_wait_result(waiter, WaitResult::Ok);
            sched::wake_task(waiter);
        }
        true
    }

    pub fn count(&self) -> usize {
        let _cs = CriticalSection::enter();
        self.inner().count
    }
}
