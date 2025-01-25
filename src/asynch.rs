//! Defines `AsyncTask`, which can encapsulate a non-send future and drive it with nonblocking code

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};

use crate::task::{Nonblock, Task};

pub struct AsyncTask<F> {
    waker: Arc<AtomicWaker>,
    waker_ref: Waker,
    fut: Pin<Box<F>>,
}
impl<F> From<F> for AsyncTask<F>
where
    F: Future,
{
    fn from(fut: F) -> Self {
        let waker = Arc::new(AtomicWaker::new());
        let waker_ref = Waker::from(Arc::clone(&waker));
        let fut = Box::pin(fut);
        Self {
            waker,
            waker_ref,
            fut,
        }
    }
}
impl<F> Task for AsyncTask<F>
where
    F: Future,
{
    type Output = F::Output;
    fn drive(&mut self) -> Nonblock<Self::Output> {
        if !self.waker.check_wake() {
            return Nonblock::Idle;
        }
        let mut cx = Context::from_waker(&self.waker_ref);
        match self.fut.as_mut().poll(&mut cx) {
            Poll::Ready(x) => Nonblock::Complete(x),
            Poll::Pending => Nonblock::Active,
        }
    }
}

struct AtomicWaker {
    wake: AtomicBool,
}
impl AtomicWaker {
    fn new() -> Self {
        Self {
            wake: AtomicBool::new(true),
        }
    }
    fn check_wake(&self) -> bool {
        if self.wake.load(Ordering::Relaxed) {
            self.wake.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}
impl Wake for AtomicWaker {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake.store(true, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.wake.store(true, Ordering::Relaxed);
    }
}
