use std::{future::Future, pin::Pin};

/// A task that wraps around an in-progress future.
pub struct Task<T> {
    future: Pin<Box<dyn Future<Output = T>>>,
}

impl<T> Task<T> where T: 'static {
    /// Create a new task, wrapping a future.
    pub fn new(inner: impl 'static + Future<Output = T>) -> Self {
        let future = Box::pin(inner) as Pin<Box<dyn Future<Output = T>>>;

        Task { future }
    }
}
