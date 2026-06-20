use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, Ordering};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Msg(u64);
// perhapes we need cache line padding
// lptr, rptr is monotonic increasing counter
pub struct SpscQueue {
    cap: usize,
    ring: Box<[UnsafeCell<Msg>]>,
    lptr: AtomicU64,
    rptr: AtomicU64,
}

unsafe impl Sync for SpscQueue {}
/*
Box::new_uninit_slice(cap), zero overhead initialization
but I'm not familier about unsafe api.
 */
impl SpscQueue {
    pub fn with_capacity(cap: usize) -> Result<Self, String> {
        if cap == 0 {
            return Err("Capacity must be greater than 0".to_string());
        }

        if cap & (cap - 1) != 0 {
            return Err("Capacity must be a power of 2".to_string());
        }
        let cap_size = cap.try_into().unwrap();
        let ring = (0..cap_size)
            .map(|_| UnsafeCell::new(Msg(0)))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(SpscQueue {
            cap,
            ring,
            lptr: AtomicU64::new(0),
            rptr: AtomicU64::new(0),
        })
    }

    pub fn try_push(&self, item: Msg) -> Result<(), Msg> {
        let rptr = self.rptr.load(Ordering::SeqCst);
        let lptr = self.lptr.load(Ordering::SeqCst);
        if rptr - lptr == self.cap as u64 {
            return Err(item);
        }

        let idx = rptr as usize % self.cap;
        unsafe {
            *self.ring[idx].get() = item;
        };

        self.rptr.store(rptr + 1, Ordering::SeqCst);
        Ok(())
    }

    pub fn try_pop(&self) -> Option<Msg> {
        let rptr = self.rptr.load(Ordering::SeqCst);
        let lptr = self.lptr.load(Ordering::SeqCst);
        if rptr == lptr {
            return None;
        }

        let item = unsafe { *self.ring[lptr as usize % self.cap].get() };

        self.lptr.store(lptr + 1, Ordering::SeqCst);
        Some(item)
    }

    pub fn len(&self) -> usize {
        (self.rptr.load(Ordering::SeqCst) - self.lptr.load(Ordering::SeqCst)) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.rptr.load(Ordering::SeqCst) == self.lptr.load(Ordering::SeqCst)
    }
    pub fn is_full(&self) -> bool {
        self.rptr.load(Ordering::SeqCst) - self.lptr.load(Ordering::SeqCst) == self.cap as u64
    }
    pub fn capacity(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_and_non_power_of_two() {
        assert!(SpscQueue::with_capacity(0).is_err());
        assert!(SpscQueue::with_capacity(3).is_err());
        assert!(SpscQueue::with_capacity(4).is_ok());
    }

    #[test]
    fn empty_pop_returns_none_without_corrupting() {
        let q = SpscQueue::with_capacity(4).unwrap();
        assert!(q.try_pop().is_none());
        // 失敗的 pop 不可以破壞計數器
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        // 之後還要能正常運作
        assert!(q.try_push(Msg(7)).is_ok());
        assert_eq!(q.try_pop().unwrap().0, 7);
    }

    #[test]
    fn push_until_full_then_reject() {
        let q = SpscQueue::with_capacity(2).unwrap();
        assert!(q.try_push(Msg(1)).is_ok());
        assert!(q.try_push(Msg(2)).is_ok());
        assert!(q.is_full());
        assert_eq!(q.len(), 2);
        // 滿了,第三筆要被拒絕,且不可破壞計數器
        assert!(q.try_push(Msg(3)).is_err());
        assert!(q.is_full());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn fifo_order() {
        let q = SpscQueue::with_capacity(4).unwrap();
        for i in 0u64..4 {
            assert!(q.try_push(Msg(i)).is_ok());
        }
        for i in 0u64..4 {
            assert_eq!(q.try_pop().unwrap().0, i);
        }
        assert!(q.is_empty());
    }

    #[test]
    fn wrap_around_preserves_fifo() {
        let q = SpscQueue::with_capacity(4).unwrap();
        // 跑很多圈,push 一個 pop 一個,index 必須正確繞回
        for i in 0u64..100 {
            assert!(q.try_push(Msg(i)).is_ok());
            assert_eq!(q.try_pop().unwrap().0, i);
        }
        assert!(q.is_empty());
    }

    #[test]
    fn full_then_pop_then_push_reuses_slot() {
        let q = SpscQueue::with_capacity(2).unwrap();
        assert!(q.try_push(Msg(1)).is_ok());
        assert!(q.try_push(Msg(2)).is_ok());
        assert!(q.try_push(Msg(3)).is_err()); // 滿
        assert_eq!(q.try_pop().unwrap().0, 1); // 騰出一格
        assert!(q.try_push(Msg(3)).is_ok()); // 重用那一格
        assert_eq!(q.try_pop().unwrap().0, 2);
        assert_eq!(q.try_pop().unwrap().0, 3);
        assert!(q.is_empty());
    }

    // 真正的雙執行緒測試:一條 thread 推,一條 thread 收。
    // 證明跨 thread 不漏單、不重複、不亂序。
    #[test]
    fn two_threads_spsc_preserves_order_and_count() {
        use std::sync::Arc;
        use std::thread;

        const N: u64 = 1_000_000;
        let q = Arc::new(SpscQueue::with_capacity(1024).unwrap());

        let producer = {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in 0..N {
                    // 滿了就 busy-spin 重試,直到推進去
                    while q.try_push(Msg(i)).is_err() {
                        std::hint::spin_loop();
                    }
                }
            })
        };

        let consumer = {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                let mut expected = 0u64;
                while expected < N {
                    match q.try_pop() {
                        Some(msg) => {
                            // 收到的順序必須剛好是 0,1,2,... 一個不差
                            assert_eq!(msg.0, expected, "順序錯了 / 有丟單或重複");
                            expected += 1;
                        }
                        None => std::hint::spin_loop(), // 空了就等 producer
                    }
                }
                expected
            })
        };

        producer.join().unwrap();
        let received = consumer.join().unwrap();
        assert_eq!(received, N);
        assert!(q.is_empty());
    }
}
