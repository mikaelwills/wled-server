use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub struct ResamplingProgress {
    pub current: AtomicU32,
    pub total: AtomicU32,
}

impl ResamplingProgress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            current: AtomicU32::new(0),
            total: AtomicU32::new(0),
        })
    }

    pub fn start(&self, total: u32) {
        self.total.store(total, Ordering::SeqCst);
        self.current.store(0, Ordering::SeqCst);
    }

    pub fn increment(&self) {
        self.current.fetch_add(1, Ordering::SeqCst);
    }

    pub fn finish(&self) {
        self.current.store(0, Ordering::SeqCst);
        self.total.store(0, Ordering::SeqCst);
    }

    pub fn get(&self) -> (u32, u32) {
        (self.current.load(Ordering::SeqCst), self.total.load(Ordering::SeqCst))
    }

    pub fn is_active(&self) -> bool {
        self.total.load(Ordering::SeqCst) > 0
    }
}

impl Default for ResamplingProgress {
    fn default() -> Self {
        Self {
            current: AtomicU32::new(0),
            total: AtomicU32::new(0),
        }
    }
}
