use std::{cell::RefCell, marker::PhantomData, task::Waker};

use failure::Error;
use mio::{Evented, Events, Poll, PollOpt, Ready, Token};
use slab::Slab;

mod stdin;

pub use self::stdin::Stdin;

/// The reactor - part of the asynchronous runtime responsible for managing the pauses between IO
/// tasks, and waking the tasks that are ready to be run after the pauses are complete.
pub struct Reactor {
    /// Handle to inner IO loop, used to wait simultaneously for multiple IO events on a single
    /// thread.
    inner: Poll,
    /// Each IO item in the poll is given an unsigned integer token. This maps from the tokens to
    /// waker objects that can be used to notify associated tasks when they are ready.
    tokens: RefCell<Slab<Vec<Waker>>>,
}

impl Reactor {
    /// Create a new instance of the reactor, ready to be linked to IO objects.
    fn new() -> Result<Self, Error> {
        let inner = Poll::new()?;
        let tokens = RefCell::new(Slab::new());

        let output = Reactor { inner, tokens };

        Ok(output)
    }

    /// Spins this reactor. This function will block until one or more of the IO objects associated
    /// with this reactor are ready to be polled again.
    fn spin_(&self) -> Result<(), Error> {
        log::trace!("Spinning");
        let mut events = Events::with_capacity(32);

        self.inner.poll(&mut events, None)?;

        events.into_iter().for_each(|event| {
            let Token(token) = event.token();
            self.tokens.borrow()[token]
                .iter()
                .for_each(Waker::wake_by_ref);
        });

        Ok(())
    }

    /// Spins the reactor of this thread. This function will block until one or more of the IO
    /// objects associated with the reactor of this thread are ready to be polled again.
    pub fn spin() -> Result<(), Error> {
        REACTOR.with(Reactor::spin_)
    }
}

std::thread_local! {
    pub static REACTOR: Reactor = Reactor::new().unwrap();
}

/// A handle to a particular task within the reactor. Binds itself to the thread local instance of
/// the reactor, so can not be sent between threads.
pub struct Handle {
    token: Token,
    reactor: PhantomData<*const Reactor>,
}

impl Handle {
    /// Create a new handle to the reactor on this thread.
    fn new() -> Self {
        let token = REACTOR.with(|reactor| reactor.tokens.borrow_mut().insert(Vec::new()));
        let token = Token(token);

        let reactor = PhantomData;

        Handle { token, reactor }
    }

    /// Register a waker with this handle. When the IO object associated with this handle is
    /// polled, the registered waker will be notified.
    fn add_waker(&self, waker: Waker) {
        let Token(token) = self.token;
        REACTOR.with(|reactor| {
            let wakers = &mut reactor.tokens.borrow_mut()[token];

            if wakers.iter().all(|waker2| !waker2.will_wake(&waker)) {
                wakers.push(waker);
            }
        })
    }

    /// Register an IO capable device with this handle.
    fn register(&self, io: &impl Evented, interest: Ready, opts: PollOpt) -> Result<(), Error> {
        REACTOR.with(|reactor| {
            reactor.inner.register(io, self.token, interest, opts)?;
            Ok(())
        })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let Token(token) = self.token;
        REACTOR.with(|reactor| {
            reactor.tokens.borrow_mut().remove(token);
        })
    }
}
