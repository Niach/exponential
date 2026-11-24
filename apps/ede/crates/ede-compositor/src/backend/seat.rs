//! libinput's two doors to the seat (#EDE-218): it opens and closes its
//! devices through the session, from `extern "C"` callbacks. A panic cannot
//! unwind out of one: the process aborts, and the person's windows go with
//! it. That happened after a 25-minute stall: logind was restarted by its
//! watchdog, the close of a revoked device failed, libseat 0.9.2 returned -1
//! without setting `errno`, and the stale 0 read back failed an assertion
//! in rustix.
//!
//! So nothing unwinds out of these two calls, whatever the session does: a
//! failed close is a line in the journal (the descriptor is closed all the
//! same), a failed open is an errno for libinput, and a panic is caught and
//! named.

use std::{
    fmt::Display,
    os::fd::OwnedFd,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
};

/// What a caught panic said.
fn panic_text(panic: &(dyn std::any::Any + Send)) -> &str {
    panic.downcast_ref::<&str>().copied().or_else(|| panic.downcast_ref::<String>().map(String::as_str)).unwrap_or("no message")
}

/// The errno libinput gets for a failed open: the session's own when it is
/// one (1..=4095), else `fallback`.
fn open_errno(own: Option<i32>, fallback: i32) -> i32 {
    own.filter(|e| (1..4096).contains(e)).unwrap_or(fallback)
}

/// `open` (the session's open of `path`) for libinput's `open_restricted`:
/// the descriptor, or an errno in 1..=4095; `errno_of` reads the session's
/// error. Nothing unwinds out of it.
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn open_guarded<E: Display>(path: &Path, open: impl FnOnce() -> Result<OwnedFd, E>, errno_of: impl FnOnce(&E) -> Option<i32>) -> Result<OwnedFd, i32> {
    match catch_unwind(AssertUnwindSafe(open)) {
        Ok(Ok(fd)) => Ok(fd),
        Ok(Err(e)) => {
            tracing::debug!("the seat did not open {}: {e}", path.display());
            Err(open_errno(errno_of(&e), libc::EPERM))
        }
        Err(panic) => {
            tracing::error!("the seat panicked opening {} ({}): libinput goes without that device", path.display(), panic_text(&*panic));
            Err(libc::EIO)
        }
    }
}

/// `close` (the session's close, which owns the descriptor: it is closed
/// when the call ends, also when the seat failed to release it or
/// panicked) for libinput's `close_restricted`. Nothing unwinds out of it;
/// whether the seat released the device.
#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub fn close_guarded<E: Display>(close: impl FnOnce() -> Result<(), E>) -> bool {
    match catch_unwind(AssertUnwindSafe(close)) {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::warn!("the seat did not release an input device ({e}); its descriptor is closed all the same");
            false
        }
        Err(panic) => {
            tracing::error!("the seat panicked releasing an input device ({}); its descriptor is closed all the same", panic_text(&*panic));
            false
        }
    }
}

/// The session as libinput's interface (in place of smithay's
/// `LibinputSessionInterface`, which lets a panic through and drops a failed
/// close without a word).
#[cfg(feature = "udev")]
pub struct SeatInterface<S: smithay::backend::session::Session>(S);

#[cfg(feature = "udev")]
impl<S: smithay::backend::session::Session> From<S> for SeatInterface<S> {
    fn from(session: S) -> Self {
        Self(session)
    }
}

#[cfg(feature = "udev")]
impl<S: smithay::backend::session::Session> smithay::reexports::input::LibinputInterface for SeatInterface<S>
where
    S::Error: Display,
{
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        use smithay::{backend::session::AsErrno, reexports::rustix::fs::OFlags};
        open_guarded(path, || self.0.open(path, OFlags::from_bits_truncate(flags as u32)), |e| e.as_errno())
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        close_guarded(|| self.0.close(fd));
    }
}

/// The guards themselves: they run in every build (CI tests without the
/// `udev` feature).
#[cfg(test)]
mod guard_tests {
    use super::*;
    use std::os::fd::AsRawFd;

    fn null() -> OwnedFd {
        std::fs::File::open("/dev/null").unwrap().into()
    }

    fn is_open(fd: i32) -> bool {
        // SAFETY: F_GETFD on a number; nothing is touched.
        unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
    }

    /// #EDE-218 as it happened: libseat left errno 0 behind a failed
    /// ReleaseDevice, and smithay's libseat session handed it to rustix.
    fn rustix_takes_errno_zero(fd: OwnedFd) -> Result<(), String> {
        let _held_until_the_unwind = fd;
        let _ = smithay::reexports::rustix::io::Errno::from_raw_os_error(std::hint::black_box(0));
        unreachable!("rustix asserts an errno is in 1..=4095")
    }

    #[test]
    fn a_close_that_panics_or_fails_returns_and_the_descriptor_is_closed() {
        let fd = null();
        let raw = fd.as_raw_fd();
        // Before the fix this panic left through libinput's C frames and
        // aborted the compositor.
        assert!(!close_guarded(|| rustix_takes_errno_zero(fd)));
        assert!(!is_open(raw), "the descriptor outlived the panicking close");

        let fd = null();
        let raw = fd.as_raw_fd();
        assert!(!close_guarded(|| {
            drop(fd);
            Err("Device not taken")
        }));
        assert!(!is_open(raw));
        assert!(close_guarded(|| Ok::<(), String>(())));
    }

    #[test]
    fn an_open_that_fails_or_panics_is_an_errno_libinput_can_take() {
        let path = Path::new("/dev/input/event0");
        assert!(open_guarded(path, || Ok::<_, String>(null()), |_| None).is_ok());
        assert_eq!(open_guarded(path, || Err("gone".to_string()), |_| Some(libc::ENODEV)).unwrap_err(), libc::ENODEV);
        // An errno that is none (libseat's stale 0) is EPERM, never 0: the
        // input crate would hand a 0 to libinput as the descriptor.
        for none in [None, Some(0), Some(-1), Some(4096)] {
            assert_eq!(open_guarded(path, || Err("refused".to_string()), |_| none).unwrap_err(), libc::EPERM);
        }
        assert_eq!(open_guarded(path, || -> Result<OwnedFd, String> { panic!("the open panicked") }, |_| None).unwrap_err(), libc::EIO);
    }
}

/// The interface itself, with a stand-in session, and through libinput.
#[cfg(all(test, feature = "udev"))]
mod tests {
    use super::*;
    use smithay::{
        backend::session::{AsErrno, Session},
        reexports::{input::LibinputInterface, rustix::fs::OFlags},
    };
    use std::{cell::Cell, os::fd::AsRawFd, rc::Rc};

    /// What the stand-in session does when asked to close.
    #[derive(Clone, Copy)]
    enum Close {
        Fails,
        /// The #EDE-218 abort: rustix's `Errno::from_raw_os_error(0)`.
        Panics,
    }

    #[derive(Debug)]
    struct Refused(i32);
    impl Display for Refused {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "refused ({})", self.0)
        }
    }
    impl AsErrno for Refused {
        fn as_errno(&self) -> Option<i32> {
            Some(self.0)
        }
    }

    /// A session that opens /dev/null for every path and fails every close.
    struct Stall {
        close: Close,
        open_panics: bool,
        open_errno: Option<i32>,
        closes: Rc<Cell<usize>>,
    }

    impl Stall {
        fn new(close: Close) -> (Self, Rc<Cell<usize>>) {
            let closes = Rc::new(Cell::new(0));
            (Self { close, open_panics: false, open_errno: None, closes: closes.clone() }, closes)
        }
    }

    impl Session for Stall {
        type Error = Refused;

        fn open(&mut self, _: &Path, _: OFlags) -> Result<OwnedFd, Refused> {
            if self.open_panics {
                panic!("the open panicked");
            }
            match self.open_errno {
                Some(errno) => Err(Refused(errno)),
                None => Ok(std::fs::File::open("/dev/null").unwrap().into()),
            }
        }

        fn close(&mut self, _fd: OwnedFd) -> Result<(), Refused> {
            self.closes.set(self.closes.get() + 1);
            match self.close {
                Close::Fails => Err(Refused(0)),
                Close::Panics => {
                    // What smithay's libseat session did with the errno 0
                    // libseat left behind.
                    let _ = smithay::reexports::rustix::io::Errno::from_raw_os_error(std::hint::black_box(0));
                    unreachable!("rustix takes an errno of 0")
                }
            }
        }

        fn change_vt(&mut self, _: i32) -> Result<(), Refused> {
            Ok(())
        }

        fn is_active(&self) -> bool {
            true
        }

        fn seat(&self) -> String {
            "seat0".into()
        }
    }

    fn is_open(fd: i32) -> bool {
        // SAFETY: F_GETFD on a number; nothing is touched.
        unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
    }

    #[test]
    fn a_close_the_seat_fails_or_panics_in_returns_and_the_descriptor_is_closed() {
        for close in [Close::Fails, Close::Panics] {
            let (session, closes) = Stall::new(close);
            let mut interface = SeatInterface::from(session);
            let fd = interface.open_restricted(Path::new("/dev/input/event0"), libc::O_RDWR).unwrap();
            let raw = fd.as_raw_fd();
            assert!(is_open(raw));
            // Before #EDE-218 the panic left through libinput's C frames
            // here and aborted the compositor.
            interface.close_restricted(fd);
            assert_eq!(closes.get(), 1);
            assert!(!is_open(raw), "the descriptor outlived the failed close");
        }
    }

    #[test]
    fn an_open_the_seat_fails_or_panics_in_is_an_errno_for_libinput() {
        let (mut session, _) = Stall::new(Close::Fails);
        session.open_errno = Some(libc::ENODEV);
        assert_eq!(SeatInterface::from(session).open_restricted(Path::new("/dev/input/event0"), 0).unwrap_err(), libc::ENODEV);
        // An errno that is none (libseat's stale 0) is EPERM, never 0: the
        // input crate hands it to libinput as the descriptor.
        for none in [0, -1, 4096] {
            let (mut session, _) = Stall::new(Close::Fails);
            session.open_errno = Some(none);
            assert_eq!(SeatInterface::from(session).open_restricted(Path::new("/dev/input/event0"), 0).unwrap_err(), libc::EPERM);
        }
        let (mut session, _) = Stall::new(Close::Fails);
        session.open_panics = true;
        assert_eq!(SeatInterface::from(session).open_restricted(Path::new("/dev/input/event0"), 0).unwrap_err(), libc::EIO);
    }

    /// The same through libinput's own C callbacks, where an unwinding panic
    /// is an abort of the whole test binary: a path context opens /dev/null
    /// through the seat, finds no evdev device behind it and closes it
    /// again. Where udev does not know /dev/null (a container without
    /// /run/udev) libinput opens nothing, and the direct test above stands.
    #[test]
    fn libinput_s_close_callback_survives_a_panicking_seat() {
        use smithay::reexports::input::Libinput;

        let (session, _closes) = Stall::new(Close::Panics);
        let mut libinput = Libinput::new_from_path(SeatInterface::from(session));
        assert!(libinput.path_add_device("/dev/null").is_none());
        let _ = libinput.dispatch();
        drop(libinput);
    }
}
