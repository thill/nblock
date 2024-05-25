use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub enum JoinError {
    /// The output for the task was already taken
    AlreadyTaken,
    /// The task was cancelled and output will never arrive
    Cancelled,
}
impl Error for JoinError {}
impl Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyTaken => f.write_str("AlreadyTaken"),
            Self::Cancelled => f.write_str("Cancelled"),
        }
    }
}

#[derive(Debug)]
pub struct TimedOut;
impl Error for TimedOut {}
impl Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TimedOut")
    }
}

#[derive(Debug)]
pub struct BuilderError {
    reason: String,
}
impl BuilderError {
    pub fn new<I: Into<String>>(reason: I) -> Self {
        Self {
            reason: reason.into(),
        }
    }
    pub fn reason<'a>(&'a self) -> &'a str {
        &self.reason
    }
}
impl Error for BuilderError {}
impl Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("BuilderError: {}", self.reason))
    }
}
