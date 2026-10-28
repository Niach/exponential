//! ONE app's tap: a private, unmuted mono-mixdown `CATapDescription` over
//! the app's process objects, a private aggregate device holding only that
//! tap (no sub-device, so no microphone and no Bluetooth profile switch), and
//! an IOProc that downmixes each cycle and hands it over a bounded channel.

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::msg_send;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioDevicePropertyNominalSampleRate,
    kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat, AudioDeviceCreateIOProcID,
    AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    AudioHardwareCreateAggregateDevice, AudioHardwareDestroyAggregateDevice, AudioObjectID,
};
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::NSString;

use super::sys;

/// IO cycles the channel holds (~10 ms each): seconds of slack before a
/// stalled reader drops audio instead of growing memory.
const CAPACITY: usize = 512;

/// `kAudioFormatLinearPCM` / `kAudioFormatFlagIsFloat`.
const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const IS_FLOAT: u32 = 1;

struct Shared {
    tx: SyncSender<Vec<f32>>,
}

pub(super) struct Tap {
    tap: AudioObjectID,
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    shared: *mut Shared,
    destroy_tap: sys::DestroyTap,
    pub(super) rx: Receiver<Vec<f32>>,
}

// The ids are plain integers; `shared` is only touched by the IOProc (on
// Core Audio's thread) until `Drop` has stopped it.
unsafe impl Send for Tap {}

impl Tap {
    /// Tap `processes` (Core Audio process objects of one app).
    pub(super) fn open(processes: &[AudioObjectID], label: &str) -> Result<Tap, String> {
        let (create_tap, destroy_tap) = sys::tap_functions().ok_or("process taps need macOS 14.2")?;
        let class = AnyClass::get(c"CATapDescription").ok_or("CATapDescription is missing")?;
        let (tap, uid) = autoreleasepool(|_| -> Result<(AudioObjectID, String), String> {
            let numbers: Vec<Retained<AnyObject>> = processes.iter().map(|id| sys::number(i64::from(*id))).collect();
            let list = sys::array(&numbers);
            let description: Option<Retained<AnyObject>> = unsafe {
                let allocated: Allocated<AnyObject> = msg_send![class, alloc];
                msg_send![allocated, initMonoMixdownOfProcesses: &*list]
            };
            let description = description.ok_or("CATapDescription init failed")?;
            let name = NSString::from_str(&format!("Exponential meeting · {label}"));
            unsafe {
                let _: () = msg_send![&*description, setName: &*name];
                let _: () = msg_send![&*description, setPrivate: Bool::YES];
                // CATapUnmuted: the person keeps hearing the app.
                let _: () = msg_send![&*description, setMuteBehavior: 0isize];
            }
            let uuid: Retained<AnyObject> = unsafe { msg_send![&*description, UUID] };
            let uid: Retained<NSString> = unsafe { msg_send![&*uuid, UUIDString] };
            let mut tap: AudioObjectID = 0;
            let status = unsafe { create_tap(Retained::as_ptr(&description), &mut tap) };
            if status != 0 || tap == 0 {
                return Err(format!("AudioHardwareCreateProcessTap failed ({status})"));
            }
            Ok((tap, uid.to_string()))
        })?;
        let (tx, rx) = sync_channel(CAPACITY);
        // Owns the tap from here: every early return below tears down what
        // exists so far.
        let mut this = Tap {
            tap,
            device: 0,
            proc_id: None,
            shared: Box::into_raw(Box::new(Shared { tx })),
            destroy_tap,
            rx,
        };
        this.start(&uid, label)?;
        Ok(this)
    }

    fn start(&mut self, uid: &str, label: &str) -> Result<(), String> {
        let tap = self.tap;
        let format: AudioStreamBasicDescription =
            sys::get(tap, kAudioTapPropertyFormat).ok_or("the tap has no format")?;
        if format.mFormatID != LINEAR_PCM || format.mFormatFlags & IS_FLOAT == 0 || format.mBitsPerChannel != 32 {
            return Err(format!("unexpected tap format {:?}", format.mFormatID.to_be_bytes()));
        }
        let mut device: AudioObjectID = 0;
        let status = autoreleasepool(|_| {
            let sub_tap = sys::dictionary();
            sys::set(&sub_tap, kAudioSubTapUIDKey, &sys::string(uid));
            sys::set(&sub_tap, kAudioSubTapDriftCompensationKey, &sys::number(1));
            let description = sys::dictionary();
            let device_uid = format!("at.exponential.meeting.{uid}");
            sys::set(&description, kAudioAggregateDeviceUIDKey, &sys::string(&device_uid));
            sys::set(&description, kAudioAggregateDeviceNameKey, &sys::string(&format!("Exponential meeting · {label}")));
            sys::set(&description, kAudioAggregateDeviceIsPrivateKey, &sys::number(1));
            sys::set(&description, kAudioAggregateDeviceTapAutoStartKey, &sys::number(1));
            sys::set(&description, kAudioAggregateDeviceTapListKey, &sys::array(&[sub_tap]));
            // NSDictionary is toll-free bridged to CFDictionary.
            let dictionary = unsafe { &*(Retained::as_ptr(&description) as *const CFDictionary) };
            unsafe { AudioHardwareCreateAggregateDevice(dictionary, NonNull::from(&mut device)) }
        });
        if status != 0 || device == 0 {
            return Err(format!("AudioHardwareCreateAggregateDevice failed ({status})"));
        }
        self.device = device;
        let mut proc_id: AudioDeviceIOProcID = None;
        let status = unsafe {
            AudioDeviceCreateIOProcID(device, Some(io_proc), self.shared.cast(), NonNull::from(&mut proc_id))
        };
        if status != 0 || proc_id.is_none() {
            return Err(format!("AudioDeviceCreateIOProcID failed ({status})"));
        }
        self.proc_id = proc_id;
        let status = unsafe { AudioDeviceStart(device, proc_id) };
        if status != 0 {
            return Err(format!("AudioDeviceStart failed ({status})"));
        }
        Ok(())
    }

    /// The rate the IOProc delivers at (the aggregate's nominal rate, which
    /// follows the output device).
    pub(super) fn rate(&self) -> f64 {
        sys::get::<f64>(self.device, kAudioDevicePropertyNominalSampleRate)
            .filter(|rate| *rate > 0.0)
            .or_else(|| sys::get::<AudioStreamBasicDescription>(self.tap, kAudioTapPropertyFormat).map(|f| f.mSampleRate))
            .unwrap_or(48_000.0)
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            if self.proc_id.is_some() {
                AudioDeviceStop(self.device, self.proc_id);
                AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            }
            if self.device != 0 {
                AudioHardwareDestroyAggregateDevice(self.device);
            }
            (self.destroy_tap)(self.tap);
            // The IOProc is gone (Stop + DestroyIOProcID return after its
            // last call), so nothing reads `shared` any more.
            drop(Box::from_raw(self.shared));
        }
    }
}

/// Core Audio's IO thread: downmix every buffer's channels to mono and hand
/// the cycle over without blocking (a full channel drops it). Never unwinds.
unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if client.is_null() {
            return;
        }
        let shared = unsafe { &*(client as *const Shared) };
        let list = unsafe { input.as_ref() };
        let buffers = unsafe { std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize) };
        let mut mono: Vec<f32> = Vec::new();
        let mut channels = 0u32;
        for buffer in buffers {
            let width = buffer.mNumberChannels.max(1) as usize;
            if buffer.mData.is_null() {
                continue;
            }
            let samples = unsafe {
                std::slice::from_raw_parts(buffer.mData as *const f32, buffer.mDataByteSize as usize / 4)
            };
            let frames = samples.len() / width;
            if mono.is_empty() {
                mono.resize(frames, 0.0);
            }
            for (frame, out) in mono.iter_mut().enumerate().take(frames) {
                *out += samples[frame * width..frame * width + width].iter().sum::<f32>();
            }
            channels += width as u32;
        }
        if mono.is_empty() || channels == 0 {
            return;
        }
        if channels > 1 {
            let scale = 1.0 / channels as f32;
            mono.iter_mut().for_each(|sample| *sample *= scale);
        }
        let _ = shared.tx.try_send(mono);
    }));
    0
}
