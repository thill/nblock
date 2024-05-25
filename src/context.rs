/// Private module, encapsulates threadlocal management of `RuntimeContext`
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use spinning_top::Spinlock;

use crate::{
    hook::{OnThreadStart, OnThreadStop},
    selector::ThreadSelector,
    thread::{ThreadController, ThreadSpawner},
};

pub struct RuntimeContext {
    pub runtime_id: u64,
    pub thread_selector: Box<dyn ThreadSelector>,
    pub thread_spawner: ThreadSpawner,
    pub runflag: Arc<AtomicBool>,
    pub controllers: Spinlock<HashMap<u64, ThreadController>>,
    pub active_task_count: Arc<AtomicUsize>,
    pub thread_wind_down_duration: Option<Duration>,
    pub thread_start_hook: Option<Box<dyn OnThreadStart>>,
    pub thread_stop_hook: Option<Box<dyn OnThreadStop>>,
}
impl RuntimeContext {
    pub fn set_threadlocal(context: Option<Arc<RuntimeContext>>) {
        CONTEXT.with_borrow_mut(|x| *x = context);
    }
    pub fn get_threadlocal() -> Option<Arc<RuntimeContext>> {
        CONTEXT.with_borrow(|x| x.clone())
    }
    pub fn next_runtime_id() -> u64 {
        NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed)
    }
}

static NEXT_RUNTIME_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CONTEXT: RefCell<Option<Arc<RuntimeContext>>> = RefCell::new(None);
}
