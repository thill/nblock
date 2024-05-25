/// Defines the `IdleStrategy` trait and various implementations
use std::{sync::Arc, thread, time::Duration};

/// Used to throttle CPU usage when all tasks on a thread are in an idle state
pub trait Idle {
    /// Called repeatedly to throttle CPU usage, incrementing `iteration` with each call.
    /// When a task becomes active again, `iteration` will reset to 0 the next time it is called.
    fn idle(&mut self, iteration: usize);
}
impl<F: FnMut(usize)> Idle for F {
    fn idle(&mut self, iteration: usize) {
        self(iteration)
    }
}

/// Used to instantiate instances of [`Idle`].
///
/// Any `Idle` impl that is `Clone + Send + Sync` automatically implements `CreateIdle` by cloning itself.
pub trait CreateIdle: Send + Sync {
    fn create_idle(&self) -> Box<dyn Idle>;
}
impl<T: Idle + Clone + Send + Sync + 'static> CreateIdle for T {
    fn create_idle(&self) -> Box<dyn Idle> {
        Box::new(self.clone())
    }
}

/// Used to reduce `Arc::new(Box::new(..))` in places that required a `Arc<Box<dyn CreateIdle>>`
pub trait IntoCreateIdle {
    fn into_create_idle(self) -> Arc<Box<dyn CreateIdle>>;
}
impl<T: CreateIdle + 'static> IntoCreateIdle for T {
    fn into_create_idle(self) -> Arc<Box<dyn CreateIdle>> {
        Arc::new(Box::new(self))
    }
}
impl IntoCreateIdle for Box<dyn CreateIdle> {
    fn into_create_idle(self) -> Arc<Box<dyn CreateIdle>> {
        Arc::new(self)
    }
}
impl IntoCreateIdle for Arc<Box<dyn CreateIdle>> {
    fn into_create_idle(self) -> Arc<Box<dyn CreateIdle>> {
        self
    }
}

/// Never idle, busy spin.
#[derive(Debug, Clone)]
pub struct NoOp;
impl Idle for NoOp {
    fn idle(&mut self, _: usize) {}
}

/// Idle with [`thread::yield_now`]
#[derive(Debug, Clone)]
pub struct Yield;
impl Idle for Yield {
    fn idle(&mut self, _: usize) {
        thread::yield_now();
    }
}

/// Idle with [`thread::sleep`]
#[derive(Debug, Clone)]
pub struct Sleep {
    duration: Duration,
}
impl Sleep {
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }
}
impl Idle for Sleep {
    fn idle(&mut self, _: usize) {
        thread::sleep(self.duration)
    }
}

/// Idle by incrementally backing off.
///
/// 1. busy spin for some iterations
/// 2. yield for some iterations
/// 3. park for some iterations, incrementally parking for longer and longer durations until the maximum is reached
#[derive(Debug, Clone)]
pub struct Backoff {
    yield_iteration_threshold: usize,
    park_iteration_threshold: usize,
    min_park_nanos: u64,
    max_park_nanos: u64,
}
impl Backoff {
    pub fn new(
        spin_iterations: usize,
        yield_iterations: usize,
        min_park_duration: Duration,
        max_park_duration: Duration,
    ) -> Self {
        Self {
            yield_iteration_threshold: spin_iterations,
            park_iteration_threshold: spin_iterations + yield_iterations,
            min_park_nanos: min_park_duration.as_nanos().try_into().unwrap_or(u64::MAX),
            max_park_nanos: max_park_duration.as_nanos().try_into().unwrap_or(u64::MAX),
        }
    }
}
impl Idle for Backoff {
    fn idle(&mut self, iteration: usize) {
        if iteration < self.yield_iteration_threshold {
            // NoOp
        } else if iteration < self.park_iteration_threshold {
            std::thread::yield_now();
        } else {
            let park_iteration = (iteration - self.park_iteration_threshold + 1) as u64;
            let park_nanos =
                std::cmp::min(self.min_park_nanos * park_iteration, self.max_park_nanos);
            std::thread::park_timeout(Duration::from_nanos(park_nanos));
        }
    }
}
impl Default for Backoff {
    fn default() -> Self {
        Self::new(16, 16, Duration::from_micros(1), Duration::from_millis(1))
    }
}
