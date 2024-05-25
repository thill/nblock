use std::{
    sync::Arc,
    thread,
    time::{Duration, SystemTime},
};

use crossbeam_utils::atomic::AtomicCell;
use nblock::{idle::NoOp, selector::DedicatedThreadSelector, task::Nonblock, Runtime};

#[test]
fn test_join_handle_try_take() {
    let runtime = create_runtime();
    let task = move || Nonblock::Complete("success".to_owned());
    let handle = runtime.spawn("mytask", move || task);
    let output = unwrap_timeout(move || handle.try_take().unwrap());
    assert_eq!("success", output.as_str());
}

#[test]
fn test_join_handle_join() {
    let runtime = create_runtime();
    let task = move || Nonblock::Complete("success".to_owned());
    let handle = runtime.spawn("mytask", move || task);
    let output = handle.join(NoOp).unwrap();
    assert_eq!("success", output.as_str());
}

#[test]
fn test_join_handle_on_complete_from_task_thread() {
    let runtime = create_runtime();
    // create task that will be sleeping while on_complete_hook is set
    let task = move || {
        thread::sleep(Duration::from_millis(100));
        Nonblock::Complete("success".to_owned())
    };
    let output = Arc::new(AtomicCell::<Option<String>>::new(None));
    {
        let output = Arc::clone(&output);
        // run the task and immediately set the on_complete hook, so the task should still be running
        runtime
            .spawn("mytask", move || task)
            .on_complete(move |x| output.store(Some(x)));
    }
    let output = unwrap_timeout(move || output.take());
    assert_eq!("success", output.as_str());
}

#[test]
fn test_join_handle_on_complete_from_caller_thread() {
    let runtime = create_runtime();
    // create task that will execute immediately
    let task = move || Nonblock::Complete("success".to_owned());
    // execute the task and get the handle
    let handle = runtime.spawn("mytask", move || task);
    // sleep, which will cause the task to execute fully
    thread::sleep(Duration::from_millis(100));
    // set on_complete, which should be for a completed task by now, which will mean the only way it can be called is from the current thread
    let output = Arc::new(AtomicCell::<Option<String>>::new(None));
    {
        let output = Arc::clone(&output);
        handle.on_complete(move |x| output.store(Some(x)));
    }
    // take output
    let output = unwrap_timeout(move || output.take());
    assert_eq!("success", output.as_str());
}

fn create_runtime() -> Runtime {
    Runtime::builder()
        .with_thread_selector(DedicatedThreadSelector::new(NoOp))
        .build()
        .unwrap()
}

fn unwrap_timeout<T, F: FnMut() -> Option<T>>(mut func: F) -> T {
    let end = SystemTime::now() + Duration::from_secs(5);
    while SystemTime::now() < end {
        if let Some(x) = func() {
            return x;
        }
        thread::sleep(Duration::from_millis(1))
    }
    func().unwrap()
}
