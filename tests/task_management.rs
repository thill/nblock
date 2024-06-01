use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime},
};

use nblock::{
    idle::NoOp,
    selector::{ConfiguredSelector, DedicatedThreadSelector, ThreadSelect},
    task::{Nonblock, Task},
    Runtime,
};
use regex::Regex;

#[test]
fn test_active_task_count() {
    let runtime = create_runtime();

    let thread1_task1_running = Arc::new(AtomicBool::new(true));
    let thread1_task2_running = Arc::new(AtomicBool::new(true));
    let thread2_task1_running = Arc::new(AtomicBool::new(true));

    let thread1_task1 = IdleUntilStopped::new(&thread1_task1_running);
    let thread1_task2 = IdleUntilStopped::new(&thread1_task2_running);
    let thread2_task1 = IdleUntilStopped::new(&thread2_task1_running);

    runtime.spawn("thread1_task1", thread1_task1);
    runtime.spawn("thread1_task2", thread1_task2);
    runtime.spawn("thread2_task1", thread2_task1);

    assert_timeout(
        || runtime.active_task_count() == 3,
        "runtime task_count != 3",
    );
    assert_timeout(
        || runtime.thread_task_count(1) == 2,
        "thread1 task_count != 2",
    );
    assert_timeout(
        || runtime.thread_task_count(2) == 1,
        "thread2 task_count != 1",
    );

    thread2_task1_running.store(false, Ordering::Relaxed);
    assert_timeout(
        || runtime.active_task_count() == 2,
        "runtime task_count != 2",
    );
    assert_timeout(
        || runtime.thread_task_count(1) == 2,
        "thread1 task_count != 2",
    );
    assert_timeout(
        || runtime.thread_task_count(2) == 0,
        "thread2 task_count != 0",
    );

    thread1_task1_running.store(false, Ordering::Relaxed);
    assert_timeout(
        || runtime.active_task_count() == 1,
        "runtime task_count != 1",
    );
    assert_timeout(
        || runtime.thread_task_count(1) == 1,
        "thread1 task_count != 1",
    );
    assert_timeout(
        || runtime.thread_task_count(2) == 0,
        "thread2 task_count != 0",
    );

    runtime
        .shutdown(NoOp, Some(Duration::from_secs(1)))
        .unwrap();
}

#[test]
fn test_panic_decrements_active_task_count() {
    let runtime = create_runtime();

    let thread1_task1_running = Arc::new(AtomicBool::new(true));
    let thread1_task2_running = Arc::new(AtomicBool::new(true));
    let thread2_task1_running = Arc::new(AtomicBool::new(true));

    let thread1_task1 = IdleUntilStopped::new(&thread1_task1_running);
    let thread1_task2 = IdleUntilStopped::new(&thread1_task2_running);
    let thread2_task1 = IdleUntilStopped::new(&thread2_task1_running);

    runtime.spawn("thread1_task1", thread1_task1);
    runtime.spawn("thread1_task2", thread1_task2);
    runtime.spawn("thread2_task1", thread2_task1);

    assert_timeout(
        || runtime.active_task_count() == 3,
        "runtime task_count != 3",
    );

    // panic thread 1, crashing new task and existing 2 tasks on thread
    runtime.spawn("thread1_task3", PanicTask);

    assert_timeout(
        || runtime.active_task_count() == 1,
        "runtime task_count != 1",
    );
    assert_timeout(
        || runtime.thread_task_count(1) == 0,
        "thread1 task_count != 0",
    );
    assert_timeout(
        || runtime.thread_task_count(2) == 1,
        "thread2 task_count != 1",
    );

    runtime
        .shutdown(NoOp, Some(Duration::from_secs(1)))
        .unwrap();
}

fn assert_timeout<F: FnMut() -> bool>(mut func: F, fail_msg: &str) {
    let timeout_at = SystemTime::now() + Duration::from_secs(5);
    while SystemTime::now() < timeout_at {
        if func() {
            return;
        }
        thread::sleep(Duration::from_millis(1));
    }
    if !func() {
        panic!("{}", fail_msg);
    }
}

struct IdleUntilStopped {
    running: Arc<AtomicBool>,
}
impl IdleUntilStopped {
    pub fn new(running: &Arc<AtomicBool>) -> Self {
        Self {
            running: Arc::clone(running),
        }
    }
}
impl Task for IdleUntilStopped {
    type Output = ();
    fn drive(&mut self) -> Nonblock<Self::Output> {
        if self.running.load(Ordering::Relaxed) {
            Nonblock::Idle
        } else {
            Nonblock::Complete(())
        }
    }
}

struct PanicTask;
impl Task for PanicTask {
    type Output = ();
    fn drive(&mut self) -> Nonblock<Self::Output> {
        panic!("panic!");
    }
}

fn create_runtime() -> Runtime {
    Runtime::builder()
        .with_thread_selector(
            ConfiguredSelector::builder()
                .with_thread_pattern(
                    Regex::new("thread1_*").unwrap(),
                    ThreadSelect::shared(1, NoOp),
                )
                .with_thread_pattern(
                    Regex::new("thread2_*").unwrap(),
                    ThreadSelect::shared(2, NoOp),
                )
                .with_fallback(DedicatedThreadSelector::new(NoOp))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
