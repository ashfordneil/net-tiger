use std::{
    sync::mpsc::SyncSender,
    task::{self, RawWaker, RawWakerVTable},
};

/// An implementation of the Waker interface used in asynchronous runtimes.
#[derive(Debug, Clone)]
pub struct Waker {
    /// The channel through which to send notification events.
    pub sender: SyncSender<usize>,
    /// The ID of the task associated with this particular waker.
    pub id: usize,
}

impl Waker {
    /// Make a copy of the waker.
    unsafe fn clone(raw: *const ()) -> RawWaker {
        let waker = &*(raw as *const Waker);
        let waker = waker.clone();
        waker.to_raw_waker()
    }

    /// Wake the waker, consuming it.
    unsafe fn wake(raw: *const ()) {
        let waker = Box::from_raw(raw as *mut Waker);
        waker.do_wake();
    }

    /// Wake the waker, without consuming it.
    unsafe fn wake_by_ref(raw: *const ()) {
        let waker = &*(raw as *const Waker);
        waker.do_wake();
    }

    /// Drop the waker.
    unsafe fn drop(raw: *const ()) {
        let waker = Box::from_raw(raw as *mut Waker);
    }

    /// The v table necessary for dynamic waker dispatch.
    const V_TABLE: RawWakerVTable =
        RawWakerVTable::new(Self::clone, Self::wake, Self::wake_by_ref, Self::drop);

    /// Create a raw waker from this waker, ready for use in std::task functions.
    fn to_raw_waker(self) -> RawWaker {
        let waker = Box::new(self);
        let waker = Box::into_raw(waker);
        RawWaker::new(waker as *const (), &Self::V_TABLE)
    }

    /// Create a real waker from this waker, ready for use in std::task functions.
    pub fn to_waker(self) -> task::Waker {
        let raw = self.to_raw_waker();
        unsafe { task::Waker::from_raw(raw) }
    }

    /// Actually wake the waker.
    fn do_wake(&self) {
        self.sender.send(self.id).unwrap();
    }
}
