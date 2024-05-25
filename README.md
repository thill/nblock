# nblock

## Description

nblock is a non-blocking runtime for Rust.
It executes non-blocking tasks on a collection of managed threads.

## Tasks

A [`Task`] is spawned on the [`Runtime`] using [`Runtime::spawn`].
Tasks are similar to a [`std::future::Future`], except that they are mutable, guaranteed to run from a single thread, and differentiate between `Idle` and `Active` states while being driven to completion.
Like a `Future`, a `Task` has an `Output`, which is able to be obtained using the [`JoinHandle`] returned by [`Runtime::spawn`].

## Threads

Tasks are spawned onto a set of shared threads that are managed by the runtime.
A tasks is bound to a specific thread based on the configured [`ThreadSelector`].

## Examples

### Round-Robin Spawn

The following example will use a Round-Robin [`ThreadSelector`] to alternate runnings tasks between two threads, printing "hello, world" and the thread name for each task.

#### Code

```rust
use nblock::{
    idle::{Backoff, NoOp},
    selector::RoundRobinSelector,
    task::{Nonblock, Task},
    Runtime,
};
use std::thread::current;

let runtime = Runtime::builder()
.with_thread_selector(
   RoundRobinSelector::builder()
       .with_thread_ids(vec![1, 2])
       .with_idle(Backoff::default())
       .build()
       .unwrap(),
)
.build()
.unwrap();

struct HelloWorldTask;
impl Task for HelloWorldTask {
    type Output = ();
    fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
        println!("hello, world from thread {:?}!", current().name().unwrap());
        Nonblock::Complete(())
    }
}

runtime.spawn("t1", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t2", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t3", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t4", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t5", || HelloWorldTask).join(NoOp).unwrap();
```

#### Output

```
hello, world from thread "nblock thread-1"!
hello, world from thread "nblock thread-2"!
hello, world from thread "nblock thread-1"!
hello, world from thread "nblock thread-2"!
hello, world from thread "nblock thread-1"!
```

### Dedicated Spawn

The following example will use a [`ThreadSelector`] that spawns each task onto a dedicated thread, printing "hello, world" and the thread name for each task.

#### Code
```rust
use nblock::{
    idle::{Backoff, NoOp},
    task::{Nonblock, Task},
    selector::DedicatedThreadSelector,
    Runtime,
};
use std::thread::current;

let runtime = Runtime::builder()
    .with_thread_selector(DedicatedThreadSelector::new(Backoff::default()))
    .build()
    .unwrap();

struct HelloWorldTask;
impl Task for HelloWorldTask {
    type Output = ();
    fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
        println!("hello, world from thread {:?}!", current().name().unwrap());
        Nonblock::Complete(())
    }
}

runtime.spawn("t1", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t2", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t3", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t4", || HelloWorldTask).join(NoOp).unwrap();
runtime.spawn("t5", || HelloWorldTask).join(NoOp).unwrap();
```

#### Output

```
hello, world from thread "nblock task t1"!
hello, world from thread "nblock task t2"!
hello, world from thread "nblock task t3"!
hello, world from thread "nblock task t4"!
hello, world from thread "nblock task t5"!
```

### Spawn On Completion

The following example shows how to use a closure on the [`JoinHandle`] to automatically spawn a new task upon completion of another.
This should feel very similar to `await` in async code, except it is lock-free while supporting task mutability.
Also notice that within a task you may use `Runtime::get()` to obtain the current Runtime.

```rust
use nblock::{
    idle::{Backoff, NoOp},
    selector::RoundRobinSelector,
    task::{Nonblock, Task},
    Runtime,
};
use std::{thread, time::Duration};

let runtime = Runtime::builder()
.with_thread_selector(
    RoundRobinSelector::builder()
        .with_thread_ids(vec![1, 2])
        .with_idle(Backoff::default())
        .build()
        .unwrap(),
)
.build()
.unwrap();

struct HelloWorldTask {
input: u64,
}
impl HelloWorldTask {
fn new(input: u64) -> Self {
    Self { input }
}
}
impl Task for HelloWorldTask {
type Output = u64;
fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
    println!(
        "Hello, world from thread {:?}! The input was {}.",
        thread::current().name().unwrap(),
        self.input
    );
    Nonblock::Complete(self.input + 1)
}
}

runtime
    .spawn("t1", move || HelloWorldTask::new(1))
    .on_complete(|output| {
        Runtime::get().spawn("t2", move || HelloWorldTask::new(output));
    });

thread::sleep(Duration::from_millis(100));

runtime
    .shutdown(NoOp, Some(Duration::from_secs(1)))
    .unwrap();
```

### Output

```
Hello, world from thread "nblock thread-1"! The input was 1.
Hello, world from thread "nblock thread-2"! The input was 2.
```

License: MIT OR Apache-2.0
