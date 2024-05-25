/// Defines the `ThreadSelector` trait and provides some implementations.
use std::{
    collections::{BTreeSet, HashMap},
    fmt::Debug,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use regex::Regex;

use crate::{
    idle::{CreateIdle, IntoCreateIdle},
    BuilderError,
};

/// Used by [`crate::Runtime::spawn`] to determine which thread should handle the task.
pub trait ThreadSelector: Send + Sync {
    fn select(&self, task_name: &str) -> ThreadSelect;
}

/// The result of [`ThreadSelector::select`]
#[derive(Clone)]
pub enum ThreadSelect {
    /// Spawn the task on a new dedicated thread where it will be the only task that will ever run on the spawned thread.
    Dedicated(Arc<Box<dyn CreateIdle>>),
    /// Spawn the task on a shared non-blocking thread as indicated by the `u64` thread ID
    Shared(u64, Arc<Box<dyn CreateIdle>>),
}
impl ThreadSelect {
    /// helper function to create a [`ThreadSelect::Dedicated`] using [`IntoCreateIdle`]
    pub fn dedicated<I: IntoCreateIdle>(idle: I) -> Self {
        Self::Dedicated(idle.into_create_idle())
    }
    /// helper function to create a [`ThreadSelect::Shared`] using a thread ID and [`IntoCreateIdle`]
    pub fn shared<I: IntoCreateIdle>(thread_id: u64, idle: I) -> Self {
        Self::Shared(thread_id, idle.into_create_idle())
    }
}
impl Debug for ThreadSelect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dedicated(_) => f.write_str("Dedicated"),
            Self::Shared(thread_id, _) => f
                .debug_struct("Shared")
                .field("thread_id", thread_id)
                .finish(),
        }
    }
}

/// Always returns [`ThreadSelect::Dedicated`]
pub struct DedicatedThreadSelector {
    idle: Arc<Box<dyn CreateIdle>>,
}
impl DedicatedThreadSelector {
    pub fn new<I: IntoCreateIdle>(idle: I) -> Self {
        Self {
            idle: idle.into_create_idle(),
        }
    }
}
impl ThreadSelector for DedicatedThreadSelector {
    fn select(&self, _task_name: &str) -> ThreadSelect {
        ThreadSelect::Dedicated(Arc::clone(&self.idle))
    }
}

/// Always returns [`ThreadSelect::Shared`], using a round-robin strategy to dispatch the load
pub struct RoundRobinSelector {
    idle: Arc<Box<dyn CreateIdle>>,
    thread_ids: Vec<u64>,
    next_seq: AtomicUsize,
}
impl RoundRobinSelector {
    pub fn builder() -> RoundRobinSelectorBuilder {
        RoundRobinSelectorBuilder::new()
    }
}
impl ThreadSelector for RoundRobinSelector {
    fn select(&self, _task_name: &str) -> ThreadSelect {
        let idx = self.next_seq.fetch_add(1, Ordering::Relaxed) % self.thread_ids.len();
        ThreadSelect::Shared(
            *self.thread_ids.get(idx).expect("thread_id"),
            Arc::clone(&self.idle),
        )
    }
}

/// Used to instantiate a [`RoundRobinSelector`], created using [`RoundRobinSelector::builder`]
pub struct RoundRobinSelectorBuilder {
    idle: Option<Arc<Box<dyn CreateIdle>>>,
    thread_ids: BTreeSet<u64>,
}
impl RoundRobinSelectorBuilder {
    fn new() -> Self {
        Self {
            idle: None,
            thread_ids: BTreeSet::new(),
        }
    }
    /// Set the [`CreateIdle`] to be used by all threads
    pub fn with_idle<I: IntoCreateIdle>(mut self, idle: I) -> Self {
        self.idle = Some(idle.into_create_idle());
        self
    }
    /// Add the given thread_id to round-robin
    pub fn add_thread_id(mut self, thread_id: u64) -> Self {
        self.thread_ids.insert(thread_id);
        self
    }
    /// Add the given thread_ids to round-robin
    pub fn add_thread_ids(mut self, thread_id: Vec<u64>) -> Self {
        self.thread_ids.extend(thread_id);
        self
    }
    /// Set the thread_ids to round-robin
    pub fn with_thread_ids(mut self, thread_id: Vec<u64>) -> Self {
        self.thread_ids.extend(thread_id);
        self
    }
    /// Build the [`RoundRobinSelector`]
    pub fn build(self) -> Result<RoundRobinSelector, BuilderError> {
        let idle = self
            .idle
            .ok_or_else(|| BuilderError::new("idle not defined"))?;
        Ok(RoundRobinSelector {
            idle,
            thread_ids: self.thread_ids.into_iter().collect(),
            next_seq: AtomicUsize::new(0),
        })
    }
}

/// Uses a set of pre-configured rules to determine which thread to select.
///
/// ## Order of Operations
/// 1. if an exact match by task name is defined, use it
/// 2. check the task name against each regex pattern in the order they were defined, using the first pattern to match
/// 3. fallback to the given fallback selector
pub struct ConfiguredSelector {
    exact_matches: HashMap<String, ThreadSelect>,
    patterns: Vec<(Regex, ThreadSelect)>,
    fallback: Box<dyn ThreadSelector>,
}
impl ConfiguredSelector {
    /// Create a new [`ConfiguredSelectorBuilder`]
    pub fn builder() -> ConfiguredSelectorBuilder {
        ConfiguredSelectorBuilder::new()
    }
}
impl ThreadSelector for ConfiguredSelector {
    fn select(&self, task_name: &str) -> ThreadSelect {
        if let Some(select) = self.exact_matches.get(task_name) {
            return select.clone();
        }
        for (regex, select) in self.patterns.iter() {
            if regex.is_match(task_name) {
                return select.clone();
            }
        }
        self.fallback.select(task_name)
    }
}

/// Used to instantiate a [`ConfiguredSelector`], created using [`ConfiguredSelector::builder`]
pub struct ConfiguredSelectorBuilder {
    exact_matches: HashMap<String, ThreadSelect>,
    patterns: Vec<(Regex, ThreadSelect)>,
    fallback: Option<Box<dyn ThreadSelector>>,
}
impl ConfiguredSelectorBuilder {
    fn new() -> Self {
        Self {
            exact_matches: HashMap::new(),
            patterns: Vec::new(),
            fallback: None,
        }
    }
    /// Set the [`ThreadSelect`] to use for the given explicit task name.
    pub fn with_pinned_task<I: Into<String>>(
        mut self,
        task_name: I,
        thread_select: ThreadSelect,
    ) -> Self {
        self.exact_matches.insert(task_name.into(), thread_select);
        self
    }
    /// Add a thread-name RegEx pattern and the [`ThreadSelect`] that should be used for names that match it.
    pub fn with_thread_pattern(mut self, pattern: Regex, thread_select: ThreadSelect) -> Self {
        self.patterns.push((pattern, thread_select));
        self
    }
    /// Set the fallback selector to use when a thread name does not match any configured pattern
    pub fn with_fallback<T>(mut self, fallback: T) -> Self
    where
        T: ThreadSelector + 'static,
    {
        self.fallback = Some(Box::new(fallback));
        self
    }
    /// Build the [`ConfiguredSelector`]
    pub fn build(self) -> Result<ConfiguredSelector, BuilderError> {
        Ok(ConfiguredSelector {
            exact_matches: self.exact_matches,
            patterns: self.patterns,
            fallback: self
                .fallback
                .ok_or_else(|| BuilderError::new("fallback selector not defined"))?,
        })
    }
}
