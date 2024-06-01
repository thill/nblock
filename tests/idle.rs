use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime},
};

use nblock::{
    idle::{Idle, Sleep},
    selector::{DedicatedThreadSelector, RoundRobinSelector},
    task::{Nonblock, Task},
    Runtime,
};

#[test]
fn test_spawned_task_idle() {
    // create task that idles 5 times before completing
    let iteration = Arc::new(AtomicUsize::new(0));
    let task = {
        let iteration = Arc::clone(&iteration);
        move || {
            if iteration.fetch_add(1, Ordering::Relaxed) < 4 {
                Nonblock::Idle
            } else {
                Nonblock::Complete(())
            }
        }
    };

    // create runtime that spawns a dedicated thread per task with a 100ms sleep idle
    let runtime = Runtime::builder()
        .with_thread_selector(DedicatedThreadSelector::new(Sleep::new(
            Duration::from_millis(100),
        )))
        .build()
        .unwrap();

    // spawn the task and wait until it's done
    let timeout_at = SystemTime::now() + Duration::from_secs(5);
    runtime.spawn("task", task);
    while iteration.load(Ordering::Relaxed) < 5 && SystemTime::now() < timeout_at {
        thread::sleep(Duration::from_millis(10));
    }

    // assert the task has complete and that iteration has been called 5 times
    assert_eq!(0, runtime.active_task_count());
    assert_eq!(5, iteration.load(Ordering::Relaxed));
}

#[test]
fn test_single_task_not_idling() {
    struct AlwaysActive;
    impl Task for AlwaysActive {
        type Output = ();
        fn drive(&mut self) -> Nonblock<Self::Output> {
            Nonblock::Active
        }
    }

    struct AlwaysIdle;
    impl Task for AlwaysIdle {
        type Output = ();
        fn drive(&mut self) -> Nonblock<Self::Output> {
            Nonblock::Idle
        }
    }

    #[derive(Clone)]
    struct IdleCounter {
        count: Arc<AtomicU64>,
    }
    impl Idle for IdleCounter {
        fn idle(&mut self, _iteration: usize) {
            self.count.fetch_add(1, Ordering::Relaxed);
            panic!("idle called!");
        }
    }

    let idle_counter = IdleCounter {
        count: Arc::new(AtomicU64::new(0)),
    };
    let idle_count = Arc::clone(&idle_counter.count);

    let runtime = Runtime::builder()
        .with_thread_selector(
            RoundRobinSelector::builder()
                .with_idle(idle_counter)
                .add_thread_id(1)
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    // spawn a task that is always idle and a task that is always active on the same thread
    runtime.spawn("active", AlwaysActive);
    runtime.spawn("idle", AlwaysIdle);

    // let threads run a bit
    thread::sleep(Duration::from_millis(100));

    // assert idle was never called
    assert_eq!(0, idle_count.load(Ordering::Relaxed));
}
