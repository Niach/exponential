//! #EDE-189: libinput's log through `tracing`, so through ede-observability's
//! log thread (#EDE-185). libinput's default handler `vfprintf`s to stderr
//! from `libinput_dispatch`, on the main loop, and its lines come when the
//! loop is already late ("event processing lagging behind by N ms"): when
//! the disk or the journal's pipe is slow too, so that write could block.
//!
//! input-sys binds neither `libinput_log_set_handler` nor `va_list` (its
//! bindgen run blocklists both), so they are declared here. The handler
//! never reads the `va_list`, it hands it on to `vsnprintf`: what matters
//! is how the architecture passes one, x86_64's and aarch64's here. On any
//! other architecture libinput keeps its own handler. libinput's ERROR
//! lines are `error!` lines, so with diagnostics on they are events too
//! (ede-observability's flood guard applies).

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
mod imp {
    use std::ffi::{c_char, c_int, c_uint, c_void, CStr};

    /// `enum libinput_log_priority`: DEBUG 10, INFO 20, ERROR 30 (the
    /// default priority: libinput sends ERROR alone unless told otherwise).
    const INFO: c_uint = 20;
    const ERROR: c_uint = 30;

    /// The longest line passed on, with its NUL. libinput's lines are a
    /// device name and one sentence; a longer one is cut and ends in `…`.
    const LINE_MAX: usize = 1024;

    /// System V AMD64: `va_list` is `struct __va_list_tag[1]`, so a parameter
    /// of that type is a pointer to its one element.
    #[cfg(target_arch = "x86_64")]
    mod va {
        #[repr(C)]
        pub struct Tag {
            pub gp_offset: u32,
            pub fp_offset: u32,
            pub overflow_arg_area: *mut std::ffi::c_void,
            pub reg_save_area: *mut std::ffi::c_void,
        }

        pub type VaList = *mut Tag;
    }

    /// AAPCS64 (Linux): `va_list` is this struct, by value. 32 bytes, so C
    /// passes a pointer to the caller's copy, and so does Rust's `extern "C"`.
    #[cfg(target_arch = "aarch64")]
    mod va {
        #[repr(C)]
        pub struct VaList {
            pub stack: *mut std::ffi::c_void,
            pub gr_top: *mut std::ffi::c_void,
            pub vr_top: *mut std::ffi::c_void,
            pub gr_offs: std::ffi::c_int,
            pub vr_offs: std::ffi::c_int,
        }
    }

    pub(super) use va::VaList;

    /// `libinput_log_handler`.
    #[cfg(feature = "udev")]
    pub(super) type Handler = unsafe extern "C" fn(*mut c_void, c_uint, *const c_char, VaList);

    extern "C" {
        fn vsnprintf(buf: *mut c_char, size: usize, format: *const c_char, args: VaList) -> c_int;
    }

    /// The handler libinput calls: the line, formatted into a bounded buffer
    /// on the stack, to `tracing` at libinput's level.
    ///
    /// # Safety
    ///
    /// `format` is a NUL-terminated printf format and `args` its arguments.
    pub(super) unsafe extern "C" fn handler(_libinput: *mut c_void, priority: c_uint, format: *const c_char, args: VaList) {
        let mut buf = [0u8; LINE_MAX];
        let n = vsnprintf(buf.as_mut_ptr().cast(), buf.len(), format, args);
        if n < 0 {
            // An encoding error: the format, unformatted, says what it was about.
            return emit(priority, &String::from_utf8_lossy(CStr::from_ptr(format).to_bytes()));
        }
        let written = CStr::from_bytes_until_nul(&buf).map_or(&buf[..0], CStr::to_bytes);
        let mut line = String::from_utf8_lossy(written).into_owned();
        if n as usize >= buf.len() {
            line.push('…');
        }
        emit(priority, &line);
    }

    fn emit(priority: c_uint, line: &str) {
        let line = line.trim_end();
        match priority {
            p if p >= ERROR => tracing::error!(target: "libinput", "{line}"),
            p if p >= INFO => tracing::info!(target: "libinput", "{line}"),
            _ => tracing::debug!(target: "libinput", "{line}"),
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use std::{
            fmt::Debug,
            sync::{Arc, Mutex},
        };

        use tracing::{
            field::{Field, Visit},
            Level, Subscriber,
        };
        use tracing_subscriber::{layer::Context, prelude::*, Layer};

        use super::*;

        type Lines = Arc<Mutex<Vec<(Level, String, String)>>>;

        /// Every event's level, target and message.
        struct Capture(Lines);

        impl<S: Subscriber> Layer<S> for Capture {
            fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
                struct Message(String);
                impl Visit for Message {
                    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
                        if field.name() == "message" {
                            self.0 = format!("{value:?}");
                        }
                    }
                }
                let mut message = Message(String::new());
                event.record(&mut message);
                self.0.lock().unwrap().push((*event.metadata().level(), event.metadata().target().to_owned(), message.0));
            }
        }

        /// The lines `f` logs (this thread's subscriber meanwhile).
        pub(crate) fn captured(f: impl FnOnce()) -> Vec<(Level, String, String)> {
            let lines = Lines::default();
            tracing::subscriber::with_default(tracing_subscriber::registry().with(Capture(lines.clone())), f);
            let lines = lines.lock().unwrap().clone();
            lines
        }

        /// `handler` called as libinput calls it: `format` with `args`, each
        /// an integer or a pointer in its own 8-byte slot. The `va_list`
        /// says every register argument is used up, so `vsnprintf` takes
        /// them all from the stack area, which both ABIs read in 8-byte slots.
        fn call(priority: c_uint, format: &CStr, args: &mut [u64]) {
            let stack = args.as_mut_ptr().cast();
            #[cfg(target_arch = "x86_64")]
            let mut tag = va::Tag { gp_offset: 6 * 8, fp_offset: 6 * 8 + 16 * 16, overflow_arg_area: stack, reg_save_area: std::ptr::null_mut() };
            #[cfg(target_arch = "x86_64")]
            let args: VaList = &mut tag;
            #[cfg(target_arch = "aarch64")]
            let args = VaList { stack, gr_top: std::ptr::null_mut(), vr_top: std::ptr::null_mut(), gr_offs: 0, vr_offs: 0 };
            // SAFETY: the format's conversions match the slots.
            unsafe { handler(std::ptr::null_mut(), priority, format.as_ptr(), args) };
        }

        fn line(level: Level, message: &str) -> (Level, String, String) {
            (level, "libinput".to_owned(), message.to_owned())
        }

        #[test]
        fn a_libinput_line_is_a_tracing_line_at_its_level() {
            let device = c"event5 - Logitech USB Receiver";
            let lines = captured(|| {
                call(ERROR, c"%s: client bug: event processing lagging behind by %dms, your system is too slow\n", &mut [device.as_ptr() as u64, 23]);
                call(INFO, c"%s: device is a keyboard\n", &mut [device.as_ptr() as u64]);
                call(10, c"%d devices\n", &mut [2]);
            });
            assert_eq!(
                lines,
                [
                    line(Level::ERROR, "event5 - Logitech USB Receiver: client bug: event processing lagging behind by 23ms, your system is too slow"),
                    line(Level::INFO, "event5 - Logitech USB Receiver: device is a keyboard"),
                    line(Level::DEBUG, "2 devices"),
                ]
            );
        }

        #[test]
        fn a_line_longer_than_the_buffer_is_cut() {
            let long = std::ffi::CString::new("x".repeat(3 * LINE_MAX)).unwrap();
            let lines = captured(|| call(ERROR, c"quirks: %s\n", &mut [long.as_ptr() as u64]));
            let [(Level::ERROR, _, message)] = &lines[..] else { panic!("one error line: {lines:?}") };
            assert_eq!(message.len(), LINE_MAX - 1 + '…'.len_utf8());
            assert!(message.starts_with("quirks: xxx") && message.ends_with("x…"), "{message}");
        }
    }
}

/// Sends `libinput`'s log lines to `tracing` instead of stderr (before
/// `udev_assign_seat`, whose device probing logs too).
#[cfg(feature = "udev")]
pub fn route(libinput: &smithay::reexports::input::Libinput) {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        use smithay::reexports::input::AsRaw as _;
        extern "C" {
            fn libinput_log_set_handler(libinput: *mut std::ffi::c_void, handler: Option<imp::Handler>);
        }
        // SAFETY: a live context; the handler is a plain function, valid for
        // the process's life.
        unsafe { libinput_log_set_handler(libinput.as_raw_mut().cast(), Some(imp::handler)) };
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = libinput;
}

/// libinput's own lines, end to end: its `va_list`, made by a real variadic
/// call in C. A path context and a path that is no device: nothing opened.
#[cfg(all(test, feature = "udev", any(target_arch = "x86_64", target_arch = "aarch64")))]
#[test]
fn libinput_s_own_lines_come_through_the_handler() {
    use smithay::reexports::input::{Libinput, LibinputInterface};
    use std::{os::fd::OwnedFd, path::Path};

    struct NoDevices;
    impl LibinputInterface for NoDevices {
        fn open_restricted(&mut self, _: &Path, _: i32) -> Result<OwnedFd, i32> {
            Err(-2)
        }
        fn close_restricted(&mut self, _: OwnedFd) {}
    }

    let lines = imp::tests::captured(|| {
        let mut libinput = Libinput::new_from_path(NoDevices);
        route(&libinput);
        assert!(libinput.path_add_device("/nonexistent/event0").is_none());
    });
    let invalid = (tracing::Level::ERROR, "libinput".to_owned(), "client bug: Invalid path /nonexistent/event0".to_owned());
    assert!(lines.contains(&invalid), "{lines:?}");
}
