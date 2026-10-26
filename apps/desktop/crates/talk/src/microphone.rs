//! NEW-78: the microphone probe behind the device doctor's `microphone` row
//! (`coding::device_doctor::add_microphone`; the IDE maps [`Microphone`] onto
//! it). Cheap, never prompting, never opening a capture stream: it runs on
//! every doctor build.
//!
//! - Linux: an input source exists (`pactl list short sources` minus the
//!   `.monitor` sources; no pactl but `pw-record` = Granted). No OS
//!   permission to ask for.
//! - macOS: `AVCaptureDevice`'s authorization for audio plus a default input
//!   device. [`request_access`] asks while the OS has not asked yet.
//! - Windows: cpal's default input device. No permission asked here.

/// What this machine's microphone looks like to the doctor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Microphone {
    /// macOS: access granted; elsewhere: an input device is there.
    Granted,
    /// macOS only: denied, restricted, or not asked yet.
    NotGranted,
    /// No input device at all.
    NoDevice,
}

/// Probe this machine (never prompts).
pub fn probe() -> Microphone {
    platform::probe()
}

/// NEW-78: the OS REFUSED the microphone (macOS: denied or restricted).
/// Not asked yet is not refused: the first capture asks.
pub fn refused() -> bool {
    platform::refused()
}

/// macOS: ask for microphone access when the OS has not asked yet; `answered`
/// gets the person's answer. `false` = nothing was asked (already decided,
/// another OS, or a build without `NSMicrophoneUsageDescription`, where
/// asking would abort the process): open System Settings instead.
pub fn request_access(answered: impl Fn(bool) + Send + Sync + 'static) -> bool {
    platform::request_access(answered)
}

/// `pactl list short sources` → the state: one source that is not a
/// `.monitor` of an output is an input device.
pub fn parse_pactl_sources(output: &str) -> Microphone {
    let input = output.lines().any(|line| {
        line.split('\t')
            .nth(1)
            .is_some_and(|name| !name.trim().is_empty() && !name.trim().ends_with(".monitor"))
    });
    if input {
        Microphone::Granted
    } else {
        Microphone::NoDevice
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{parse_pactl_sources, Microphone};
    use std::process::{Command, Stdio};

    pub(super) fn probe() -> Microphone {
        let output = Command::new("pactl")
            .args(["list", "short", "sources"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(output) if output.status.success() => {
                parse_pactl_sources(&String::from_utf8_lossy(&output.stdout))
            }
            // No pactl (or no server answering it): a PipeWire recorder
            // on the PATH is the best this can say without opening a stream.
            _ if on_path("pw-record") => Microphone::Granted,
            _ => Microphone::NoDevice,
        }
    }

    fn on_path(binary: &str) -> bool {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join(binary).is_file())
        })
    }

    pub(super) fn refused() -> bool {
        false
    }

    pub(super) fn request_access(_answered: impl Fn(bool) + Send + Sync + 'static) -> bool {
        false
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Microphone;
    use block2::RcBlock;
    use objc2::rc::autoreleasepool;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::msg_send;
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: &'static NSString;
    }

    /// `AVAuthorizationStatus`.
    const NOT_DETERMINED: isize = 0;
    const AUTHORIZED: isize = 3;

    fn capture_device() -> Option<&'static AnyClass> {
        AnyClass::get(c"AVCaptureDevice")
    }

    fn status(class: &AnyClass) -> isize {
        unsafe { msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] }
    }

    pub(super) fn probe() -> Microphone {
        let Some(class) = capture_device() else {
            return Microphone::NoDevice;
        };
        autoreleasepool(|_| {
            let device: *mut AnyObject =
                unsafe { msg_send![class, defaultDeviceWithMediaType: AVMediaTypeAudio] };
            if device.is_null() {
                Microphone::NoDevice
            } else if status(class) == AUTHORIZED {
                Microphone::Granted
            } else {
                Microphone::NotGranted
            }
        })
    }

    /// Asking without the usage string in the main bundle's Info.plist
    /// aborts the process (TCC), so an unbundled dev build never asks.
    fn has_usage_description() -> bool {
        let Some(bundle_class) = AnyClass::get(c"NSBundle") else {
            return false;
        };
        autoreleasepool(|_| {
            let key = NSString::from_str("NSMicrophoneUsageDescription");
            let bundle: *mut AnyObject = unsafe { msg_send![bundle_class, mainBundle] };
            if bundle.is_null() {
                return false;
            }
            let value: *mut AnyObject =
                unsafe { msg_send![bundle, objectForInfoDictionaryKey: &*key] };
            !value.is_null()
        })
    }

    /// `AVAuthorizationStatusRestricted` (1) or `Denied` (2).
    pub(super) fn refused() -> bool {
        capture_device().is_some_and(|class| matches!(status(class), 1 | 2))
    }

    pub(super) fn request_access(answered: impl Fn(bool) + Send + Sync + 'static) -> bool {
        let Some(class) = capture_device() else {
            return false;
        };
        if status(class) != NOT_DETERMINED || !has_usage_description() {
            return false;
        }
        let block = RcBlock::new(move |granted: Bool| answered(granted.as_bool()));
        unsafe {
            let _: () = msg_send![
                class,
                requestAccessForMediaType: AVMediaTypeAudio,
                completionHandler: &*block
            ];
        }
        true
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::Microphone;
    use cpal::traits::HostTrait as _;

    pub(super) fn probe() -> Microphone {
        if cpal::default_host().default_input_device().is_some() {
            Microphone::Granted
        } else {
            Microphone::NoDevice
        }
    }

    pub(super) fn refused() -> bool {
        false
    }

    pub(super) fn request_access(_answered: impl Fn(bool) + Send + Sync + 'static) -> bool {
        false
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::Microphone;

    pub(super) fn probe() -> Microphone {
        Microphone::NoDevice
    }

    pub(super) fn refused() -> bool {
        false
    }

    pub(super) fn request_access(_answered: impl Fn(bool) + Send + Sync + 'static) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_source_is_an_input_device() {
        let output = "\
49\talsa_output.pci-0000_00_1f.3.analog-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED
50\talsa_input.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED
";
        assert_eq!(parse_pactl_sources(output), Microphone::Granted);
    }

    #[test]
    fn only_monitors_or_nothing_is_no_device() {
        let monitors = "49\talsa_output.pci-0000_00_1f.3.hdmi-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tIDLE\n";
        assert_eq!(parse_pactl_sources(monitors), Microphone::NoDevice);
        assert_eq!(parse_pactl_sources(""), Microphone::NoDevice);
        assert_eq!(parse_pactl_sources("garbage without tabs\n"), Microphone::NoDevice);
    }
}
