//! The raw pieces: Core Audio object properties, the macOS 14.2+ symbols
//! looked up at run time (the IDE installs on 13.0, where a static import of
//! `AudioHardwareCreateProcessTap` would refuse to launch), libproc paths and
//! the private TCC preflight.

use std::ffi::{c_char, c_void, CStr};
use std::mem::size_of;
use std::ptr::{null, null_mut, NonNull};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_core_audio::{
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertySelector,
};
use objc2_foundation::NSString;

extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn proc_pidpath(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    fn sysctlbyname(
        name: *const c_char,
        old: *mut c_void,
        old_len: *mut usize,
        new: *mut c_void,
        new_len: usize,
    ) -> i32;
}

const RTLD_LAZY: i32 = 0x1;
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

pub(super) type CreateTap = unsafe extern "C" fn(*const AnyObject, *mut AudioObjectID) -> i32;
pub(super) type DestroyTap = unsafe extern "C" fn(AudioObjectID) -> i32;

/// `AudioHardwareCreateProcessTap` / `AudioHardwareDestroyProcessTap`
/// (macOS 14.2+), `None` on older systems.
pub(super) fn tap_functions() -> Option<(CreateTap, DestroyTap)> {
    let create = unsafe { dlsym(RTLD_DEFAULT, c"AudioHardwareCreateProcessTap".as_ptr()) };
    let destroy = unsafe { dlsym(RTLD_DEFAULT, c"AudioHardwareDestroyProcessTap".as_ptr()) };
    if create.is_null() || destroy.is_null() {
        return None;
    }
    Some(unsafe {
        (std::mem::transmute::<*mut c_void, CreateTap>(create), std::mem::transmute::<*mut c_void, DestroyTap>(destroy))
    })
}

/// `kern.osproductversion` as (major, minor).
pub(super) fn os_version() -> Option<(u32, u32)> {
    let mut buffer = [0u8; 32];
    let mut len = buffer.len();
    let status = unsafe {
        sysctlbyname(c"kern.osproductversion".as_ptr(), buffer.as_mut_ptr().cast(), &mut len, null_mut(), 0)
    };
    if status != 0 {
        return None;
    }
    let text = CStr::from_bytes_until_nul(&buffer).ok()?.to_str().ok()?;
    let mut parts = text.split('.').map(|part| part.parse::<u32>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0)))
}

/// The executable path of `pid`.
pub(super) fn pid_path(pid: i32) -> Option<String> {
    let mut buffer = vec![0u8; 4096];
    let len = unsafe { proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if len <= 0 {
        return None;
    }
    buffer.truncate(len as usize);
    String::from_utf8(buffer).ok()
}

/// The process macOS holds responsible for `pid` (an XPC service's client
/// app), through the libquarantine SPI every app links; `None` = unknown.
pub(super) fn responsible_pid(pid: i32) -> Option<i32> {
    let symbol = unsafe { dlsym(RTLD_DEFAULT, c"responsibility_get_pid_responsible_for_pid".as_ptr()) };
    if symbol.is_null() {
        return None;
    }
    let function: unsafe extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(symbol) };
    let responsible = unsafe { function(pid) };
    (responsible > 0).then_some(responsible)
}

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// One fixed-size property value (`T` = a plain C value, valid all-zero).
pub(super) fn get<T: Copy>(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<T> {
    let address = address(selector);
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0 && size as usize == size_of::<T>()).then_some(value)
}

/// A property that is a list of object ids.
pub(super) fn get_ids(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Vec<AudioObjectID> {
    let address = address(selector);
    let mut size = 0u32;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(object, NonNull::from(&address), 0, null(), NonNull::from(&mut size)) };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut ids = vec![0 as AudioObjectID; size as usize / size_of::<AudioObjectID>()];
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(ids.as_mut_ptr()).cast(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    ids
}

/// A CFString property (returned +1 retained; released here).
pub(super) fn get_string(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<String> {
    let raw = get::<usize>(object, selector)?;
    let string = unsafe { Retained::from_raw(raw as *mut NSString) }?;
    let text = string.to_string();
    (!text.is_empty()).then_some(text)
}

/// The private TCC framework's `TCCAccessPreflight` / `TCCAccessRequest`
/// (what AudioCap and the system's own clients use): the Audio Recording
/// permission has no public preflight.
pub(super) struct Tcc {
    preflight: unsafe extern "C" fn(*const AnyObject, *const c_void) -> i32,
    request: unsafe extern "C" fn(*const AnyObject, *const c_void, *const c_void),
}

/// `kTCCServiceAudioCapture`.
pub(super) const AUDIO_CAPTURE: &str = "kTCCServiceAudioCapture";
/// `kTCCServiceScreenCapture` (Screen & System Audio Recording, full).
pub(super) const SCREEN_CAPTURE: &str = "kTCCServiceScreenCapture";

impl Tcc {
    pub(super) fn load() -> Option<Tcc> {
        let handle = unsafe { dlopen(c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC".as_ptr(), RTLD_LAZY) };
        if handle.is_null() {
            return None;
        }
        let preflight = unsafe { dlsym(handle, c"TCCAccessPreflight".as_ptr()) };
        let request = unsafe { dlsym(handle, c"TCCAccessRequest".as_ptr()) };
        if preflight.is_null() || request.is_null() {
            return None;
        }
        Some(unsafe { Tcc { preflight: std::mem::transmute(preflight), request: std::mem::transmute(request) } })
    }

    /// 0 = granted, 1 = denied, anything else = not asked yet.
    pub(super) fn preflight(&self, service: &str) -> i32 {
        let service = NSString::from_str(service);
        unsafe { (self.preflight)(Retained::as_ptr(&service).cast(), null()) }
    }

    /// Ask (shows the OS prompt while not asked yet); `answered` runs on a
    /// TCC queue.
    pub(super) fn request(&self, service: &str, answered: impl Fn(bool) + 'static) {
        let service = NSString::from_str(service);
        let block = block2::RcBlock::new(move |granted: u8| answered(granted != 0));
        unsafe { (self.request)(Retained::as_ptr(&service).cast(), null(), (&*block as *const block2::Block<dyn Fn(u8)>).cast()) }
    }
}

/// `+[NSMutableDictionary new]`.
pub(super) fn dictionary() -> Retained<AnyObject> {
    unsafe { msg_send![class!(NSMutableDictionary), new] }
}

pub(super) fn set(dictionary: &AnyObject, key: &CStr, value: &AnyObject) {
    let key = NSString::from_str(key.to_str().unwrap_or_default());
    unsafe {
        let _: () = msg_send![dictionary, setObject: value, forKey: &*key];
    }
}

pub(super) fn number(value: i64) -> Retained<AnyObject> {
    unsafe { msg_send![class!(NSNumber), numberWithLongLong: value] }
}

pub(super) fn string(value: &str) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(NSString::from_str(value)))
}

pub(super) fn array(items: &[Retained<AnyObject>]) -> Retained<AnyObject> {
    let array: Retained<AnyObject> = unsafe { msg_send![class!(NSMutableArray), new] };
    for item in items {
        unsafe {
            let _: () = msg_send![&*array, addObject: &**item];
        }
    }
    array
}
