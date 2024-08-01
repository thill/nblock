//! # Description
//!
//! nblock is a non-blocking runtime for Rust.
//! It executes non-blocking tasks on a collection of managed threads.
//!
//! # Tasks
//!
//! A [`Task`] is spawned on the [`Runtime`] using [`Runtime::spawn`].
//! Tasks are similar to a [`std::future::Future`], except that they are mutable, guaranteed to run from a single thread, and differentiate between `Idle` and `Active` states while being driven to completion.
//! Like a `Future`, a `Task` has an `Output`, which is able to be obtained using the [`JoinHandle`] returned by [`Runtime::spawn`].
//!
//! # Threads
//!
//! Tasks are spawned onto a set of shared threads that are managed by the runtime.
//! A tasks is bound to a specific thread based on the provided [`ThreadSelector`].
//!
//! # Examples
//!
//! ## Round-Robin Spawn
//!
//! The following example will use a Round-Robin [`ThreadSelector`] to alternate runnings tasks between two threads, printing "hello, world" and the thread name for each task.
//!
//! ### Code
//!
//! ```
//! use nblock::{
//!     idle::{Backoff, NoOp},
//!     selector::RoundRobinSelector,
//!     task::{Nonblock, Task},
//!     Runtime,
//! };
//! use std::thread::current;
//!
//!let runtime = Runtime::builder()
//!    .with_thread_selector(
//!        RoundRobinSelector::builder()
//!            .with_thread_ids(vec![1, 2])
//!            .with_idle(Backoff::default())
//!            .build()
//!            .unwrap(),
//!    )
//!    .build()
//!    .unwrap();
//!
//! struct HelloWorldTask;
//! impl Task for HelloWorldTask {
//!     type Output = ();
//!     fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
//!         println!("hello, world! from: {:?}", current().name().unwrap());
//!         Nonblock::Complete(())
//!     }
//! }
//!
//! runtime.spawn("t1", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t2", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t3", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t4", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t5", HelloWorldTask).join(NoOp).unwrap();
//! ```
//!
//! ### Output
//!
//! ```text
//! hello, world! from: "nblock thread-1"
//! hello, world! from: "nblock thread-2"
//! hello, world! from: "nblock thread-1"
//! hello, world! from: "nblock thread-2"
//! hello, world! from: "nblock thread-1"
//! ```
//!
//! ## Dedicated Spawn
//!
//! The following example will use a [`ThreadSelector`] that spawns each task onto a dedicated thread, printing "hello, world" and the thread name for each task.
//!
//! ### Code
//! ```
//! use nblock::{
//!     idle::{Backoff, NoOp},
//!     task::{Nonblock, Task},
//!     selector::DedicatedThreadSelector,
//!     Runtime,
//! };
//! use std::thread::current;
//!
//! let runtime = Runtime::builder()
//!     .with_thread_selector(DedicatedThreadSelector::new(Backoff::default()))
//!     .build()
//!     .unwrap();
//!
//! struct HelloWorldTask;
//! impl Task for HelloWorldTask {
//!     type Output = ();
//!     fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
//!         println!("hello, world! from: {:?}", current().name().unwrap());
//!         Nonblock::Complete(())
//!     }
//! }
//!
//! runtime.spawn("t1", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t2", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t3", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t4", HelloWorldTask).join(NoOp).unwrap();
//! runtime.spawn("t5", HelloWorldTask).join(NoOp).unwrap();
//! ```
//!
//! ### Output
//!
//! ```text
//! hello, world! from: "nblock task t1"
//! hello, world! from: "nblock task t2"
//! hello, world! from: "nblock task t3"
//! hello, world! from: "nblock task t4"
//! hello, world! from: "nblock task t5"
//! ```
//!
//! ## Spawn On Completion
//!
//! The following example shows how to use a closure on the [`JoinHandle`] to automatically spawn a new task upon completion of another.
//! This should feel very similar to `await` in async code, except it is lock-free while supporting task mutability.
//! Also notice that within a task you may use `Runtime::get()` to obtain the current Runtime.
//!
//! ```
//! use nblock::{
//!     idle::{Backoff, NoOp},
//!     selector::RoundRobinSelector,
//!     task::{Nonblock, Task},
//!     Runtime,
//! };
//! use std::{thread, time::Duration};
//!
//! let runtime = Runtime::builder()
//!     .with_thread_selector(
//!         RoundRobinSelector::builder()
//!             .with_thread_ids(vec![1, 2])
//!             .with_idle(Backoff::default())
//!             .build()
//!             .unwrap(),
//!     )
//!     .build()
//!     .unwrap();
//!
//! struct HelloWorldTask {
//!     input: u64,
//! }
//! impl HelloWorldTask {
//!     fn new(input: u64) -> Self {
//!         Self { input }
//!     }
//! }
//! impl Task for HelloWorldTask {
//!     type Output = u64;
//!     fn drive(&mut self) -> nblock::task::Nonblock<Self::Output> {
//!         println!(
//!             "hello, world! from: {:?}! The input was {}.",
//!             thread::current().name().unwrap(),
//!             self.input
//!         );
//!         Nonblock::Complete(self.input + 1)
//!     }
//! }
//!
//! runtime
//!     .spawn("t1", HelloWorldTask::new(1))
//!     .on_complete(|output| {
//!         Runtime::get().spawn("t2", HelloWorldTask::new(output));
//!     });
//!
//! thread::sleep(Duration::from_millis(100));
//!
//! runtime
//!     .shutdown(NoOp, Some(Duration::from_secs(1)))
//!     .unwrap();
//! ```
//!
//! ## Output
//!
//! ```text
//! hello, world! from: "nblock thread-1"! The input was 1.
//! hello, world! from: "nblock thread-2"! The input was 2.
//! ```

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use context::RuntimeContext;
use error::{BuilderError, JoinError, TimedOut};
use hook::{OnTaskComplete, OnThreadStart, OnThreadStop};
use idle::{Idle, IntoCreateIdle};
use selector::{ThreadSelect, ThreadSelector};
use spinning_top::Spinlock;
use task::{IntoTask, Task};
use thread::{JoinHandleContext, JoinHandleState, ThreadController, ThreadSpawner};

pub mod error;
pub mod hook;
pub mod idle;
pub mod selector;
pub mod task;

mod context;
mod thread;

/// A non-blocking runtime, which will use a given [`ThreadSelector`] to assign [`Task`]s to threads.
///
/// ## Threads
///
/// The [`ThreadSelector`] will assign a spawned [`Task`] to a thread.
/// - [`ThreadSelect::Dedicated`] will assign the task to its own, new, dedicated thread, which will terminate as soon as the task is shutdown.
/// - [`ThreadSelect::Shared`] will assign the task to a shared thread by ID. Shared threads will terminate after all of its tasks have completed plus a given [`RuntimeBuilder::thread_wind_down_duration`].
///
/// ## Run Flag
///
/// The [`RuntimeBuilder::runflag`] will be used to determine if underlying tasks should continue to be polled.
/// One the `runflag` is set to false, all tasks will immediately stop being polled and threads will gracefully terminate and [`Drop`] all active tasks.
pub struct Runtime {
    context: Arc<RuntimeContext>,
}
impl Runtime {
    /// Create a [`RuntimeBuilder`] to start building a new [`Runtime`]
    pub fn builder() -> RuntimeBuilder {
        RuntimeBuilder::new()
    }

    /// Set the thread-local [`Runtime`] as the current instance.
    ///
    /// Threads started by [`Runtime::spawn`] will automatically assign the thread-local runtime instance immediately upon being spawend.
    pub fn set_threadlocal<I: Into<Arc<Runtime>>>(runtime: &Runtime) {
        RuntimeContext::set_threadlocal(Some(Arc::clone(&runtime.context)))
    }

    /// Set the thread-local [`Runtime`] to [`None`]
    pub fn unset_threadlocal() {
        RuntimeContext::set_threadlocal(None)
    }

    /// Get the thread-local [`Runtime`], returning [`None`] if it did not exist.
    pub fn get_threadlocal() -> Option<Runtime> {
        RuntimeContext::get_threadlocal().map(|context| Runtime { context })
    }

    /// Get the current thread-local [`Runtime`], panicing if it did not exist.
    pub fn get() -> Runtime {
        Self::get_threadlocal().expect("runtime not defined for current thread")
    }

    /// Get the total number of active tasks across all threads, including dedicated threads.
    ///
    /// Note: The active_task_count is incremented and decremented by the task thread, so pending tasks will not increment the count until discovered by the selected thread.
    pub fn active_task_count(&self) -> usize {
        self.context.active_task_count.load(Ordering::Relaxed)
    }

    /// Get the number of active tasks on the given shared thread id
    ///
    /// Note: The active_task_count is incremented and decremented by the task thread, so pending tasks will not increment the count until discovered by the selected thread.
    pub fn thread_task_count(&self, thread_id: u64) -> usize {
        self.context
            .controllers
            .lock()
            .get(&thread_id)
            .map(|x| x.active_task_count())
            .unwrap_or(0)
    }

    /// Get the flag that is used to check if the runtime should keep running.
    ///
    /// Setting this atomic flag to false anywhere in the codebase will cause this runtime to stop running.
    /// There are no guarantees a [`Task`] will have [`Task::drive`] called again after this flag is set to false.
    pub fn runflag<'a>(&'a self) -> &'a Arc<AtomicBool> {
        &self.context.runflag
    }

    /// Spawn the given task on a nonblocking thread.
    ///
    /// ## Threading
    ///
    /// The underlying [`ThreadSelector`] will determine which thread the task runs on.
    /// Multiple non-blocking tasks can run concurrently on the same threa where they will be serviced in a round-robin fashion.
    /// A single nonblocking thread will not attempt to idle unless all active tasks report [`task::Nonblock::Idle`].
    ///
    /// ## Task + Send
    ///
    /// For tasks that are [`Send`], you may pass `|| task` into this function to spawn it.
    ///
    /// ## Task + !Send
    ///
    /// Since a [`Task`] will always run on a single thread, it is possible to avoid requiring it to be [`Send`].
    /// Instead [`IntoTask`] will be sent to the target nonblocking thread, where it will instantiate the [`Task`] there.
    /// This allows a user to create non-[`Send`] tasks by transferring the [`Send`] responsibility to the [`IntoTask`].
    ///
    /// ## IntoTask
    ///
    /// Users are free to implement [`IntoTask`] for their custom types, which reduces boilerplate code needed to spawn nonblocking tasks through this function.
    pub fn spawn<T>(&self, task_name: &str, task: T) -> JoinHandle<<T::Task as Task>::Output>
    where
        T: IntoTask + Send + 'static,
    {
        let thread_select = self.context.thread_selector.select(task_name);
        match thread_select {
            ThreadSelect::Dedicated(idle) => {
                let controller = ThreadController::new();
                let join_handle = controller.add_task(task);
                self.context.thread_spawner.spawn(
                    task_name,
                    Arc::clone(&self.context),
                    idle,
                    &controller,
                    None,
                );
                join_handle
            }
            ThreadSelect::Shared(thread_id, idle) => {
                let mut controllers = self.context.controllers.lock();
                match controllers.get_mut(&thread_id) {
                    Some(controller) => controller.add_task(task),
                    None => {
                        let controller = controllers
                            .entry(thread_id)
                            .or_insert_with(|| ThreadController::new());
                        let join_handle = controller.add_task(task);
                        self.context.thread_spawner.spawn(
                            task_name,
                            Arc::clone(&self.context),
                            idle,
                            controller,
                            Some(thread_id),
                        );
                        join_handle
                    }
                }
            }
        }
    }

    /// Shutdown the runtime by setting the `runflag` to false.
    /// Then wait for it to terminate, optionally timing-out after the given `timeout` [`Duration`].
    ///
    /// This function blocks, which means it is not appropriate to call from within a non-blocking [`Task`].
    pub fn shutdown<I: Idle>(&self, idle: I, timeout: Option<Duration>) -> Result<(), TimedOut> {
        self.context.runflag.store(false, Ordering::Release);
        let timeout = timeout.map(|x| SystemTime::now() + x);
        let mut idle = idle;
        let mut idle_iteration = 0;
        while self.context.active_task_count.load(Ordering::Relaxed) > 0
            && timeout.map(|x| SystemTime::now() < x).unwrap_or(true)
        {
            idle.idle(idle_iteration);
            idle_iteration += 1;
        }
        if self.context.active_task_count.load(Ordering::Relaxed) == 0 {
            Ok(())
        } else {
            Err(TimedOut)
        }
    }

    /// Join the runtime, returning after a graceful exit.
    ///
    /// This function blocks, which means it is not appropriate to call from within a non-blocking [`Task`].
    pub fn join<I: Idle>(&self, idle: I) {
        let mut idle = idle;
        let mut idle_iteration = 0;
        while self.context.runflag.load(Ordering::Relaxed)
            && self.context.active_task_count.load(Ordering::Relaxed) > 0
        {
            idle.idle(idle_iteration);
            idle_iteration += 1;
        }
    }
}

/// Builds a [`Runtime`], instantiated using [`Runtime::builder`]
///
/// ## Parameters and Defaults
///
/// - thread_selector: the strategy used to assign tasks to threads (no default, [`Runtime::builder`] will return an [`Err`] if not set)
/// - runflag: allows the user to pass in a shared runflag (defaults to a new [`AtomicBool`])
/// - thread_wind_down_duration: amount of time a [`ThreadSelect::Shared`] thread will stay alive after its underlying tasks have completed (defaults to 1 second)
/// - thread_start_hook: called from within a spawned [`Runtime`] thread before any tasks are polled (defaults to [`None`])
/// - thread_stop_hook: called from within a spawned [`Runtime`] thread immediately before terminating after all tasks and wind-down has completed (defaults to [`None`])
pub struct RuntimeBuilder {
    thread_selector: Option<Box<dyn ThreadSelector>>,
    runflag: Option<Arc<AtomicBool>>,
    thread_wind_down_duration: Option<Duration>,
    thread_start_hook: Option<Box<dyn OnThreadStart>>,
    thread_stop_hook: Option<Box<dyn OnThreadStop>>,
}
impl RuntimeBuilder {
    fn new() -> Self {
        Self {
            thread_selector: None,
            runflag: None,
            thread_wind_down_duration: Some(Duration::from_secs(1)),
            thread_start_hook: None,
            thread_stop_hook: None,
        }
    }

    /// The strategy used to assign tasks to threads
    pub fn with_thread_selector<T>(mut self, thread_selector: T) -> Self
    where
        T: ThreadSelector + Send + Sync + 'static,
    {
        self.thread_selector = Some(Box::new(thread_selector));
        self
    }

    /// The strategy used to assign tasks to threads
    pub fn set_thread_selector<T>(&mut self, thread_selector: T) -> &mut Self
    where
        T: ThreadSelector + Send + Sync + 'static,
    {
        self.thread_selector = Some(Box::new(thread_selector));
        self
    }

    /// Use a shared runflag.
    /// When this flag is set to false, all tasks will stop being polled
    ///
    /// Defaults to a new [`AtomicBool`].
    pub fn with_runflag(mut self, runflag: Arc<AtomicBool>) -> Self {
        self.runflag = Some(runflag);
        self
    }

    /// Use a shared runflag.
    /// When this flag is set to false, all tasks will stop being polled
    ///
    /// Defaults to a new [`AtomicBool`].
    pub fn set_runflag(&mut self, runflag: Arc<AtomicBool>) -> &mut Self {
        self.runflag = Some(runflag);
        self
    }

    /// Amount of time a [`ThreadSelect::Shared`] thread will stay alive after its underlying tasks have completed.
    ///
    /// This allows a thread to stay alive for some duration, waiting to accept new tasks, preventing expensive thread-startup operations.
    ///
    /// Defaults to 1 second.
    /// Set to [`None`] to never stop shared threads.
    pub fn with_thread_wind_down_duration(
        mut self,
        thread_wind_down_duration: Option<Duration>,
    ) -> Self {
        self.thread_wind_down_duration = thread_wind_down_duration;
        self
    }

    /// Amount of time a [`ThreadSelect::Shared`] thread will stay alive after its underlying tasks have completed.
    ///
    /// This allows a thread to stay alive for some duration, waiting to accept new tasks, preventing expensive thread-startup operations.
    ///
    /// Defaults to 1 second.
    /// Set to [`None`] to never stop shared threads.
    pub fn set_thread_wind_down_duration(
        &mut self,
        thread_wind_down_duration: Option<Duration>,
    ) -> &mut Self {
        self.thread_wind_down_duration = thread_wind_down_duration;
        self
    }

    /// Called from within a spawned [`Runtime`] thread before any tasks are polled.
    /// This is useful for monitoring or to set thread affinity.
    pub fn with_thread_start_hook<T, I>(mut self, thread_start_hook: I) -> Self
    where
        T: OnThreadStart + 'static,
        I: Into<Box<T>>,
    {
        self.thread_start_hook = Some(thread_start_hook.into());
        self
    }

    /// Called from within a spawned [`Runtime`] thread before any tasks are polled.
    /// This is useful for monitoring or to set thread affinity.
    pub fn set_thread_start_hook<T, I>(&mut self, thread_start_hook: I) -> &mut Self
    where
        T: OnThreadStart + 'static,
        I: Into<Box<T>>,
    {
        self.thread_start_hook = Some(thread_start_hook.into());
        self
    }

    /// Called from within a spawned [`Runtime`] thread immediately before terminating after all tasks and wind-down has completed.
    pub fn with_thread_stop_hook<T, I>(mut self, thread_stop_hook: I) -> Self
    where
        T: OnThreadStop + 'static,
        I: Into<Box<T>>,
    {
        self.thread_stop_hook = Some(thread_stop_hook.into());
        self
    }

    /// Called from within a spawned [`Runtime`] thread immediately before terminating after all tasks and wind-down has completed.
    pub fn set_thread_stop_hook<T, I>(&mut self, thread_stop_hook: I) -> &mut Self
    where
        T: OnThreadStop + 'static,
        I: Into<Box<T>>,
    {
        self.thread_stop_hook = Some(thread_stop_hook.into());
        self
    }

    /// Build the [`Runtime`]
    pub fn build(self) -> Result<Runtime, BuilderError> {
        let thread_selector = self
            .thread_selector
            .ok_or_else(|| BuilderError::new("thread_selector not defined"))?;
        let runflag = self
            .runflag
            .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));
        let context = Arc::new(RuntimeContext {
            runtime_id: RuntimeContext::next_runtime_id(),
            thread_selector,
            thread_spawner: ThreadSpawner::new(),
            runflag,
            controllers: Spinlock::new(HashMap::new()),
            active_task_count: Arc::new(AtomicUsize::new(0)),
            thread_wind_down_duration: self.thread_wind_down_duration,
            thread_start_hook: self.thread_start_hook,
            thread_stop_hook: self.thread_stop_hook,
        });
        Ok(Runtime { context })
    }
}

/// Returned by [`Runtime::spawn`] to monitor a [`Task`] and poll its output.
pub struct JoinHandle<T> {
    context: Arc<JoinHandleContext<T>>,
}
impl<T> JoinHandle<T> {
    fn new() -> Self {
        Self {
            context: Arc::new(JoinHandleContext::new()),
        }
    }

    /// Set the [`OnTaskComplete`] hook that will be called when the task has been complete.
    /// If the task was already complete, the hook will be called immediately from the caller's thread.
    pub fn on_complete<H>(self, complete_hook: H)
    where
        H: OnTaskComplete<T> + 'static,
    {
        Arc::clone(&self.context).set_complete_hook(complete_hook, self)
    }

    /// Check if the task is still running
    pub fn is_active(&self) -> bool {
        self.context.state() == JoinHandleState::Active
    }

    /// Check if the task has been completed successfully
    pub fn is_complete(&self) -> bool {
        self.context.state() == JoinHandleState::Complete
    }

    /// Check if the task has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.context.state() == JoinHandleState::Cancelled
    }

    /// Join the task, blocking until it is complete or cancelled.
    ///
    /// ## Return Values
    /// - [`None`] - task is still running and an output is not yet available.
    /// - [`Ok`] - task has been completed and the output has been taken. Subsequent calls to take will fail with [`JoinError::AlreadyTaken`].
    /// - [`Err`] - the task has been cancelled or the result has already been taken by [`Self::try_take`].
    pub fn join<I: IntoCreateIdle>(self, idle: I) -> Result<T, JoinError> {
        let mut idle = idle.into_create_idle().create_idle();
        let mut iteration = 0;
        while self.is_active() {
            idle.idle(iteration);
            iteration += 1;
        }
        match self.context.state() {
            JoinHandleState::Complete => match self.context.take_output() {
                Some(x) => Ok(x),
                None => Err(JoinError::AlreadyTaken),
            },
            _ => Err(JoinError::Cancelled),
        }
    }

    /// Try to take the [`Task`] `output`.
    ///
    /// ## Return Values
    /// - [`None`] - task is still running and an output is not yet available.
    /// - [`Ok`] - task has been completed and the output has been taken. Subsequent calls to take will fail with [`JoinError::AlreadyTaken`].
    /// - [`Err`] - the task has been cancelled or the result has already been taken.
    pub fn try_take(&self) -> Result<Option<T>, JoinError> {
        match self.context.state() {
            JoinHandleState::Active => Ok(None),
            JoinHandleState::Complete => match self.context.take_output() {
                Some(x) => Ok(Some(x)),
                None => Err(JoinError::AlreadyTaken),
            },
            _ => Err(JoinError::Cancelled),
        }
    }

    /// Cancel the current task
    pub fn cancel(&self) {
        self.context.cancel();
    }
}
impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.context.orphan()
    }
}
