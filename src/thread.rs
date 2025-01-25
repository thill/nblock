//! Private module, encapsulates logic for spawning threads and managing tasks

use std::{
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime},
};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;

use crate::{
    context::RuntimeContext,
    hook::OnTaskComplete,
    idle::{CreateIdle, Idle},
    task::{IntoTask, Nonblock, Task},
    JoinHandle,
};

use self::context::{PanicHookInstaller, ThreadContext};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TaskOutcome {
    Active,
    Complete,
    Idle,
}

/// Spawns a nonblocking thread, which can consist of multiple active tasks.
pub struct ThreadSpawner {}
impl ThreadSpawner {
    pub fn new() -> Self {
        PanicHookInstaller::check_install();
        Self {}
    }
    /// When `controllers` and `thread_id` are set, the `thread_id` key will be removed from `controllers` when complete and there are no pending tasks
    pub fn spawn(
        &self,
        task_name: &str,
        context: Arc<RuntimeContext>,
        idle: Arc<Box<dyn CreateIdle>>,
        controller: &ThreadController,
        thread_id: Option<u64>,
    ) {
        let task_queue = Arc::clone(&controller.task_queue);
        let thread_task_count = Arc::clone(&controller.thread_task_count);
        let runtime_name = match context.runtime_id {
            1 => format!("nblock"),
            x => format!("nblock-{x}"),
        };
        let thread_name = match &thread_id {
            None => format!("{} task {}", runtime_name, task_name),
            Some(thread_id) => format!("{} thread-{}", runtime_name, thread_id),
        };
        thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                RuntimeContext::set_threadlocal(Some(Arc::clone(&context)));
                ThreadContext::initialize(
                    Arc::clone(&context.active_task_count),
                    thread_task_count,
                );
                if let Some(thread_start_hook) = &context.thread_start_hook {
                    thread_start_hook.on_thread_start(thread_id);
                }
                let mut group = TaskGroup::new(
                    Arc::clone(&task_queue),
                    idle.create_idle(),
                    context.thread_wind_down_duration,
                );
                while context.runflag.load(Ordering::Relaxed) {
                    if let TaskOutcome::Complete = group.drive() {
                        // no more active tasks, check if there are any pending tasks within lock, go down if thread complete and there are no pending tasks
                        match thread_id {
                            Some(thread_id) => {
                                let mut controllers = context.controllers.lock();
                                if task_queue.is_empty() {
                                    controllers.remove(&thread_id);
                                }
                            }
                            None => break,
                        }
                    }
                }
                // thread is stopping, adjust runtime active task count for any tasks that were still alive
                ThreadContext::stopped();
                // finally call the stop hook
                if let Some(thread_stop_hook) = &context.thread_stop_hook {
                    thread_stop_hook.on_thread_stop(thread_id);
                }
            })
            .expect("spawned thread");
    }
}

/// Controller that can send uninitialized [`Task`] impls to an underlying thread that will drive them
pub struct ThreadController {
    thread_task_count: Arc<AtomicUsize>,
    task_queue: Arc<SegQueue<PendingTask>>,
}
impl ThreadController {
    pub fn new() -> Self {
        Self {
            thread_task_count: Arc::new(AtomicUsize::new(0)),
            task_queue: Arc::new(SegQueue::new()),
        }
    }
    pub fn active_task_count(&self) -> usize {
        self.thread_task_count.load(Ordering::Relaxed)
    }
    pub fn add_task<T, I>(&self, task: I) -> JoinHandle<T::Output>
    where
        T: Task + 'static,
        T::Output: Send + 'static,
        I: IntoTask<Task = T> + Send + 'static,
    {
        let join_handle = JoinHandle::new();
        let join_context = Arc::clone(&join_handle.context);
        self.task_queue.push(PendingTask::new(task, join_context));
        join_handle
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinHandleState {
    Active = 0,
    Complete = 1,
    Cancelled = 2,
    Orphaned = 3,
}

struct JoinHandleCompleteHook<T> {
    _join_handle: JoinHandle<T>, // move ownership here to avoid orphaning task
    hook: Box<dyn OnTaskComplete<T>>,
}

pub struct JoinHandleContext<T> {
    // used by reader to determine if task is active, complete, or cancelled
    state: AtomicU8,
    // optionally holds the completed output to be swapped by a JoinHandle consumer
    output: AtomicCell<Option<T>>,
    // hook that will be called when the task is complete
    complete_hook: AtomicCell<Option<JoinHandleCompleteHook<T>>>,
}
impl<T> JoinHandleContext<T> {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(JoinHandleState::Active as u8),
            output: AtomicCell::new(None),
            complete_hook: AtomicCell::new(None),
        }
    }
    pub fn set_complete_hook<H: OnTaskComplete<T> + 'static>(
        &self,
        complete_hook: H,
        join_handle: JoinHandle<T>,
    ) {
        self.complete_hook.store(Some(JoinHandleCompleteHook {
            _join_handle: join_handle,
            hook: Box::new(complete_hook),
        }));
        if let Some(output) = self.take_output() {
            self.complete_hook
                .take()
                .map(|mut x| x.hook.on_task_complete(output));
        }
    }
    pub fn complete(&self, output: T) {
        // must set output before changing state to avoid race conditions
        match self.complete_hook.take() {
            Some(mut complete_hook) => complete_hook.hook.on_task_complete(output),
            None => {
                self.output.store(Some(output));
            }
        }
        self.state
            .compare_exchange(
                JoinHandleState::Active as u8,
                JoinHandleState::Complete as u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
    }
    pub fn cancel(&self) {
        self.state
            .compare_exchange(
                JoinHandleState::Active as u8,
                JoinHandleState::Cancelled as u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
    }
    pub fn orphan(&self) {
        self.state
            .compare_exchange(
                JoinHandleState::Active as u8,
                JoinHandleState::Orphaned as u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
    }
    pub fn state(&self) -> JoinHandleState {
        match self.state.load(Ordering::Relaxed) {
            0 => JoinHandleState::Active,
            1 => JoinHandleState::Complete,
            2 => JoinHandleState::Cancelled,
            _ => JoinHandleState::Orphaned,
        }
    }
    pub fn take_output(&self) -> Option<T> {
        self.output.take()
    }
}

/// Appendable group of [`Task`] impls that are driven together via a single `drive` function
struct TaskGroup {
    task_queue: Arc<SegQueue<PendingTask>>,
    idle: Box<dyn Idle>,
    wind_down_duration: Option<Duration>,
    active_tasks: Vec<TaskDriver>,
    idle_iteration: usize,
    wind_down_complete_time: Option<SystemTime>,
}
impl TaskGroup {
    fn new(
        task_queue: Arc<SegQueue<PendingTask>>,
        idle: Box<dyn Idle>,
        wind_down_duration: Option<Duration>,
    ) -> Self {
        Self {
            task_queue,
            idle,
            wind_down_duration,
            active_tasks: Vec::new(),
            idle_iteration: 0,
            wind_down_complete_time: None,
        }
    }
    pub fn drive(&mut self) -> TaskOutcome {
        let mut active = false;
        while let Some(pending_task) = self.task_queue.pop() {
            active = true;
            self.active_tasks.push((pending_task.task_provider)());
            ThreadContext::increment_task_count();
        }
        let mut task_idx = 0;
        while task_idx < self.active_tasks.len() {
            match self.active_tasks.get_mut(task_idx).unwrap().drive() {
                TaskOutcome::Active => {
                    task_idx += 1;
                    active = true;
                }
                TaskOutcome::Idle => {
                    task_idx += 1;
                }
                TaskOutcome::Complete => {
                    self.active_tasks.remove(task_idx);
                    ThreadContext::decrement_task_count();
                    active = true;
                }
            }
        }
        if self.active_tasks.is_empty() {
            // start wind-down
            match self.wind_down_duration {
                // no wind-down, never exit
                None => TaskOutcome::Idle,
                // zero wind-down, exit immediately
                Some(Duration::ZERO) => TaskOutcome::Idle,
                Some(wind_down_duration) => match self.wind_down_complete_time {
                    None => {
                        self.wind_down_complete_time = Some(SystemTime::now() + wind_down_duration);
                        TaskOutcome::Idle
                    }
                    Some(wind_down_complete_time) => {
                        if SystemTime::now() >= wind_down_complete_time {
                            TaskOutcome::Complete
                        } else {
                            self.idle.idle(self.idle_iteration);
                            self.idle_iteration += 1;
                            TaskOutcome::Idle
                        }
                    }
                },
            }
        } else if active {
            self.idle_iteration = 0;
            TaskOutcome::Active
        } else {
            self.idle.idle(self.idle_iteration);
            self.idle_iteration += 1;
            TaskOutcome::Idle
        }
    }
}

/// Encapsulates an uninitialized [`Task`] via [`IntoTask`], which is able to be sent to a [`TaskGroup`].
struct PendingTask {
    task_provider: Box<dyn FnOnce() -> TaskDriver + Send>,
}
impl PendingTask {
    fn new<T, I>(uninstantiated_task: I, join_context: Arc<JoinHandleContext<T::Output>>) -> Self
    where
        T: Task + 'static,
        T::Output: Send + 'static,
        I: IntoTask<Task = T> + Send + 'static,
    {
        Self {
            task_provider: Box::new(move || {
                TaskDriver::new(uninstantiated_task.into_task(), join_context)
            }),
        }
    }
}

/// Encapsulates a [`Task`] and [`JoinHandleContext`], driving them as a pair through dynamic dispatch
struct TaskDriver {
    drive_fn: Box<dyn FnMut() -> TaskOutcome>,
}
impl TaskDriver {
    fn new<T: Task + 'static>(mut task: T, handle: Arc<JoinHandleContext<T::Output>>) -> Self
    where
        T: Task,
    {
        let drive_fn = Box::new(move || {
            if handle.state() == JoinHandleState::Cancelled {
                return TaskOutcome::Complete;
            }
            match task.drive() {
                Nonblock::Active => TaskOutcome::Active,
                Nonblock::Idle => TaskOutcome::Idle,
                Nonblock::Complete(output) => {
                    handle.complete(output);
                    TaskOutcome::Complete
                }
            }
        });
        Self { drive_fn }
    }
    fn drive(&mut self) -> TaskOutcome {
        (self.drive_fn)()
    }
}

/// This module privately encapsulates the thread-local and static resources utilized by nblock threads.
/// Access to these resources are done through public structs within this module.
mod context {
    use std::{
        cell::RefCell,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
    };

    use once_cell::sync::Lazy;

    thread_local! {
        static THREAD_CONTEXT: RefCell<Option<ThreadContext>> = RefCell::new(None);
    }

    static PANIC_INSTALLER: Lazy<PanicHookInstaller> = Lazy::new(|| PanicHookInstaller::default());

    pub struct PanicHookInstaller {
        installed: AtomicBool,
    }
    impl Default for PanicHookInstaller {
        fn default() -> Self {
            PanicHookInstaller {
                installed: AtomicBool::new(false),
            }
        }
    }
    impl PanicHookInstaller {
        pub fn check_install() {
            if let Ok(_) = PANIC_INSTALLER.installed.compare_exchange(
                false,
                true,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                PanicHookInstaller::install();
            }
        }
        fn install() {
            let original_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                ThreadContext::stopped();
                original_hook(panic_info)
            }));
        }
    }

    pub struct ThreadContext {
        thread_active_task_count: Arc<AtomicUsize>,
        runtime_active_task_count: Arc<AtomicUsize>,
    }
    impl ThreadContext {
        pub fn initialize(
            runtime_active_task_count: Arc<AtomicUsize>,
            thread_active_task_count: Arc<AtomicUsize>,
        ) {
            THREAD_CONTEXT.with_borrow_mut(|x| {
                *x = Some(Self {
                    thread_active_task_count,
                    runtime_active_task_count,
                })
            });
        }
        pub fn increment_task_count() {
            THREAD_CONTEXT.with_borrow_mut(|x| {
                if let Some(context) = x {
                    context
                        .thread_active_task_count
                        .fetch_add(1, Ordering::Relaxed);
                    context
                        .runtime_active_task_count
                        .fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        pub fn decrement_task_count() {
            THREAD_CONTEXT.with_borrow_mut(|x| {
                if let Some(context) = x {
                    context
                        .thread_active_task_count
                        .fetch_sub(1, Ordering::Relaxed);
                    context
                        .runtime_active_task_count
                        .fetch_sub(1, Ordering::Relaxed);
                }
            });
        }
        pub fn stopped() {
            THREAD_CONTEXT.with_borrow_mut(|x| {
                if let Some(context) = x {
                    let thread_task_count =
                        context.thread_active_task_count.swap(0, Ordering::Relaxed);
                    context
                        .runtime_active_task_count
                        .fetch_sub(thread_task_count, Ordering::Release);
                }
            });
        }
    }
}
