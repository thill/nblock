/// This module contains various hook traits, with [`Fn`] or [`FnMut`] impls.

/// Called when a [`crate::task::Task`] is complete, set by [`crate::JoinHandle::on_complete`].
pub trait OnTaskComplete<T>: Send {
    fn on_task_complete(&mut self, output: T);
}
impl<T, F: FnMut(T) + Send> OnTaskComplete<T> for F {
    fn on_task_complete(&mut self, output: T) {
        self(output)
    }
}

/// Called when a new [`Runtime`] thread is started, set by [`crate::RuntimeBuilder::thread_start_hook`].
pub trait OnThreadStart: Send + Sync {
    fn on_thread_start(&self, thread_id: Option<u64>);
}
impl<F: Fn(Option<u64>) + Send + Sync> OnThreadStart for F {
    fn on_thread_start(&self, thread_id: Option<u64>) {
        self(thread_id)
    }
}

/// Called when a new [`Runtime`] thread is stopped, set by [`crate::RuntimeBuilder::thread_start_hook`].
pub trait OnThreadStop: Send + Sync {
    fn on_thread_stop(&self, thread_id: Option<u64>);
}
impl<F: Fn(Option<u64>) + Send + Sync> OnThreadStop for F {
    fn on_thread_stop(&self, thread_id: Option<u64>) {
        self(thread_id)
    }
}
