use std::{
    io::{self, ErrorKind, Read},
    os::raw::c_int,
    pin::Pin,
    task::{Context, Poll},
};

use failure::Error;
use futures::io::AsyncRead;
use mio::{unix::EventedFd, PollOpt, Ready};

use super::Handle;

// Only one handle to stdin can exist at a time. This module defines a singleton mutex.
mod lock {
    use std::sync::atomic::{AtomicBool, Ordering};

    use failure::Error;

    // If true, the mutex is locked. If false, the lock is free.
    static STDIN_LOCK: AtomicBool = AtomicBool::new(false);

    /// A guard around the locked mutex.
    pub struct Guard(());

    impl Guard {
        /// Take the mutex. Returns Err if the mutex is already taken.
        pub fn take() -> Result<Self, Error> {
            if STDIN_LOCK.compare_and_swap(false, true, Ordering::Relaxed) {
                // the lock was already taken
                failure::bail!("Stdin is already locked.");
            } else {
                Ok(Guard(()))
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            STDIN_LOCK.store(false, Ordering::Relaxed);
        }
    }
}

/// An asynchronous wrapper around stdin.
pub struct Stdin {
    // only one handle can exist at a time
    _lock: lock::Guard,
    // to reset stdin using fcntl when we are done
    old_state: c_int,
    // the stdin object itself for reading from
    inner: io::Stdin,
    // a handle to the reactor for asynchronous actions
    handle: Handle,
}

impl Stdin {
    /// Create a new wrapper around stdin.
    pub fn new() -> Result<Self, Error> {
        let _lock = lock::Guard::take()?;

        let old_state = unsafe {
            let old_state = match libc::fcntl(libc::STDIN_FILENO, libc::F_GETFD) {
                -1 => return Err(io::Error::last_os_error().into()),
                n => n,
            };
            // set stdin to not block
            match libc::fcntl(
                libc::STDIN_FILENO,
                libc::F_SETFD,
                old_state | libc::O_NONBLOCK,
            ) {
                0 => (),
                -1 => return Err(io::Error::last_os_error().into()),
                _ => unreachable!(),
            };

            old_state
        };

        let inner = io::stdin();

        let handle = Handle::new();
        handle.register(
            &EventedFd(&libc::STDIN_FILENO),
            Ready::readable(),
            PollOpt::edge(),
        )?;

        Ok(Stdin {
            _lock,
            old_state,
            inner,
            handle,
        })
    }
}

impl Drop for Stdin {
    fn drop(&mut self) {
        unsafe {
            libc::fcntl(libc::STDIN_FILENO, libc::F_SETFD, self.old_state);
        }
    }
}

impl AsyncRead for Stdin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        ctx: &mut Context,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.as_mut().inner.read(buffer) {
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                self.handle.add_waker(ctx.waker().clone());
                Poll::Pending
            }
            res => Poll::Ready(res),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        fs::File,
        io::Write,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    use futures::{
        executor,
        io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    };
    use rusty_fork::{fork, rusty_fork_id, ChildWrapper};

    use crate::{executor::Executor, reactor::Stdin};

    fn pipe_stdin(cmd: &mut Command) {
        cmd.stdin(Stdio::piped());
    }

    #[test]
    fn echo_eventually() {
        fn parent(child: &mut ChildWrapper, _: &mut File) {
            write!(child.inner_mut().stdin.take().unwrap(), "Hello, world\n").unwrap();

            let status = child.wait().unwrap();
            assert!(status.success());
        }

        fn child() {
            let mut input = Stdin::new().unwrap();
            let future = async {
                let mut buffer = String::new();
                input.read_to_string(&mut buffer).await.unwrap();
                assert_eq!("Hello, world\n", buffer);
            };

            executor::block_on(future);
        }

        fork(
            "reactor::stdin::test::echo_eventually",
            rusty_fork_id!(),
            pipe_stdin,
            parent,
            child,
        )
        .unwrap();
    }

    #[test]
    fn run_in_executor() {
        fn parent(child: &mut ChildWrapper, _: &mut File) {
            let mut pipe = child.inner_mut().stdin.take().unwrap();

            write!(pipe, "Hello, world\n").unwrap();

            thread::sleep(Duration::from_secs(5));

            write!(pipe, "Goodbye for now\n").unwrap();

            drop(pipe);

            let status = child.wait().unwrap();
            assert!(status.success());
        }

        fn child() {
            let future = async {
                let input = Stdin::new().unwrap();
                let mut input = BufReader::new(input);
                let mut buffer = String::new();

                let start = Instant::now();
                input.read_line(&mut buffer).await.unwrap();
                let time = start.elapsed();

                // if it took more than 5 seconds to read, that means that we weren't able to
                // separate the two read calls from eachother for some reason
                assert!(time < Duration::from_secs(5));

                assert_eq!("Hello, world\n", buffer);
                buffer.clear();

                input.read_line(&mut buffer).await.unwrap();
                assert_eq!("Goodbye for now\n", buffer);
                buffer.clear();

                assert_eq!(0, input.read_to_string(&mut buffer).await.unwrap());
            };

            let mut executor = Executor::new();
            executor.complete(future).unwrap();
        }

        fork(
            "reactor::stdin::test::run_in_executor",
            rusty_fork_id!(),
            pipe_stdin,
            parent,
            child,
        )
        .unwrap();
    }
}
