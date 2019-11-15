use std::{
    future::Future,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    task::{self, Context, Poll},
};

use failure::Error;
use slab::Slab;

mod waker;

use self::waker::Waker;
use crate::reactor::Reactor;

type Task<'a, T> = Pin<Box<dyn 'a + Future<Output = T>>>;

/// The executor - part of the asynchronous runtime responsible for running tasks when they need to
/// be run. The executor is the entrypoint to the runtime, and wraps all other parts of the
/// runtime.
pub struct Executor {
    to_do: Receiver<usize>,
    send_handle: SyncSender<usize>,
    tasks: Slab<MaybeUninit<(Task<'static, ()>, task::Waker)>>,
    /// In functions such as complete, we need to be able to have a separate task (that is not
    /// static, and returns a value) that is also handled by the executor. Reserve an ID in the
    /// slab that is guaranteed to not be otherwise used, and store it here.
    separate_task: usize,
    /// Ensure that we don't send the executor between threads, as it is tied to its specific
    /// thread-local reactor.
    reactor: PhantomData<*const Reactor>,
}

impl Executor {
    /// Create a new executor.
    pub fn new() -> Self {
        let (send_handle, to_do) = mpsc::sync_channel(64);
        let mut tasks = Slab::new();

        // make sure that we don't just go straight to the slab when we need new things
        let separate_task = tasks.insert(MaybeUninit::uninit());
        // We later assume that this is true in the complete function. Verify that assumption on
        // startup.
        assert_eq!(separate_task, 0);

        let reactor = PhantomData;

        Executor {
            to_do,
            send_handle,
            tasks,
            separate_task,
            reactor,
        }
    }

    /// Spawn a new future onto the executor, to be run in the background. This will only be polled
    /// during the times in which the executor is running - it does not run automatically.
    pub fn spawn(&mut self, future: impl 'static + Future<Output = ()>) {
        let future = Box::pin(future) as Task<'static, ()>;
        let space = self.tasks.vacant_entry();
        let waker = Waker {
            sender: self.send_handle.clone(),
            id: space.key(),
        }
        .to_waker();

        space.insert(MaybeUninit::new((future, waker)));
    }

    /// Run a single future to completion on the executor. Will poll any background futures while
    /// running this future, but will return as soon as the main future has finished.
    pub fn complete<'a, T>(&mut self, future: impl 'a + Future<Output = T>) -> Result<T, Error> {
        let mut main_future = Box::pin(future) as Task<'a, T>;
        let waker = Waker {
            sender: self.send_handle.clone(),
            id: self.separate_task,
        }
        .to_waker();

        // futures to remove
        let mut completed = Vec::<usize>::new();

        // start by polling every future we have, just in case.
        for (id, future) in self.tasks.iter_mut() {
            if id == self.separate_task {
                let mut ctx = Context::from_waker(&waker);
                if let Poll::Ready(result) = main_future.as_mut().poll(&mut ctx) {
                    // We assume that self.separate_task is equal to 0, meaning that this is the
                    // first time through the loop. As such, if the main future has completed we
                    // don't need to worry about removing other completed futures from the backlog,
                    // as they haven't been polled yet.
                    return Ok(result);
                }
            } else {
                // we know that future is a valid value as long as the ID isn't
                // self.separate_task, as that is the only ID in the slab associated
                // with an uninitialised value.
                let (future, waker) = unsafe { &mut *future.as_mut_ptr() };
                let mut ctx = Context::from_waker(&waker);
                if let Poll::Ready(()) = future.as_mut().poll(&mut ctx) {
                    completed.push(id);
                }
            }
        }

        // clean up any futures that finished in that initial poll round
        completed.drain(..).for_each(|i| {
            self.tasks.remove(i);
        });

        let output = loop {
            let future_to_poll = match self.to_do.try_recv() {
                Ok(id) => id,
                Err(TryRecvError::Disconnected) => {
                    unreachable!()
                }
                Err(TryRecvError::Empty) => {
                    Reactor::spin()?;
                    continue;
                }
            };

            if future_to_poll == self.separate_task {
                let mut ctx = Context::from_waker(&waker);
                if let Poll::Ready(result) = main_future.as_mut().poll(&mut ctx) {
                    break result;
                }
            } else {
                // we know that future is a valid value as long as the ID isn't
                // self.separate_task, as that is the only ID in the slab associated
                // with an uninitialised value.
                let (future, waker) = unsafe { &mut *self.tasks[future_to_poll].as_mut_ptr() };
                let mut ctx = Context::from_waker(&waker);
                if let Poll::Ready(()) = future.as_mut().poll(&mut ctx) {
                    self.tasks.remove(future_to_poll);
                }
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod test {
    use std::{future::Future, pin::Pin, task::{Context, Poll}};

    use crate::executor::Executor;

    /// A future that will pause its own execution, before continuing. The first time it is polled
    /// it will yield, and the second time it will complete.
    struct Pause(bool);

    impl Pause {
        fn new() -> Self {
            Pause(false)
        }
    }

    impl Future for Pause {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<()> {
            match self.0 {
                false => {
                    ctx.waker().wake_by_ref();
                    self.0 = true;
                    Poll::Pending
                },
                true => Poll::Ready(())
            }
        }
    }

    #[test]
    fn immediate() {
        let future = async { 5 };
        let mut executor = Executor::new();
        assert_eq!(5, executor.complete(future).unwrap());
    }

    #[test]
    fn pause_once() {
        let future = async {
            Pause::new().await;
            5
        };
        let mut executor = Executor::new();
        assert_eq!(5, executor.complete(future).unwrap());
    }

    #[test]
    fn many_pauses() {
        let future = async {
            for _ in 0..10usize {
                Pause::new().await;
            }

            5
        };
        let mut executor = Executor::new();
        assert_eq!(5, executor.complete(future).unwrap());
    }
}
