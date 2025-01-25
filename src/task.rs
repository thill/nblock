//! Definitions for task-related traits and impls

/// Represents a single non-blocking task to be spawned using [`crate::Runtime::spawn`].
///
/// A default [`FnMut`] implementation is provided.
pub trait Task {
    type Output;
    fn drive(&mut self) -> Nonblock<Self::Output>;
}
impl<O, F> Task for F
where
    F: FnMut() -> Nonblock<O>,
{
    type Output = O;
    fn drive(&mut self) -> Nonblock<Self::Output> {
        self()
    }
}

/// Something that can be converted into a [`Task`].
///
/// [`crate::Runtime::spawn`] requires that implementations be [`Send`], so that it can be dispatched to a non-blocking thread where the [`Task`] will be created.
///
/// A default `Task` impl is provided so that any `Task` can `IntoTask`.
pub trait IntoTask {
    type Task: Task;
    fn into_task(self) -> Self::Task;
}
impl<T> IntoTask for T
where
    T: Task,
{
    type Task = T;
    fn into_task(self) -> Self::Task {
        self
    }
}

/// The result of a single call to [`Task::drive`].
///
/// This is similar to a `Future` in async code, except it differentiates between a task being `Idle` or `Working` to impact duty-cycles accordingly.
pub enum Nonblock<T> {
    Active,
    Complete(T),
    Idle,
}
