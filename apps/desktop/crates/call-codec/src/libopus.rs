//! NEW-88: the reference libopus (system library, `build.rs`), through the
//! few entry points of `opus.h` the call needs. Declared by hand: no bindgen,
//! so no libclang.

use std::ffi::{c_int, c_void, CStr};
use std::ptr::NonNull;

const OPUS_APPLICATION_VOIP: c_int = 2048;
const OPUS_SET_BITRATE_REQUEST: c_int = 4002;
const OPUS_SET_INBAND_FEC_REQUEST: c_int = 4012;
const OPUS_SET_PACKET_LOSS_PERC_REQUEST: c_int = 4014;

extern "C" {
    fn opus_encoder_create(fs: i32, channels: c_int, application: c_int, error: *mut c_int) -> *mut c_void;
    fn opus_encoder_ctl(st: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_encode(st: *mut c_void, pcm: *const i16, frame_size: c_int, data: *mut u8, max_data_bytes: i32) -> i32;
    fn opus_encoder_destroy(st: *mut c_void);
    fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut c_void;
    fn opus_decode_float(st: *mut c_void, data: *const u8, len: i32, pcm: *mut f32, frame_size: c_int, decode_fec: c_int) -> c_int;
    fn opus_decoder_destroy(st: *mut c_void);
    fn opus_strerror(error: c_int) -> *const std::ffi::c_char;
}

fn error(code: c_int) -> String {
    // SAFETY: opus_strerror returns a static NUL-terminated string for any code.
    unsafe { CStr::from_ptr(opus_strerror(code)) }.to_string_lossy().into_owned()
}

fn check(code: c_int) -> Result<(), String> {
    if code < 0 {
        Err(error(code))
    } else {
        Ok(())
    }
}

pub struct Encoder(NonNull<c_void>);

// SAFETY: an OpusEncoder is a plain heap block with no thread affinity; it is
// only used through `&mut self`.
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(rate: u32, bitrate: i32, loss_percent: Option<i32>) -> Result<Self, String> {
        let mut code = 0;
        // SAFETY: valid arguments; the result is checked for NULL.
        let st = unsafe { opus_encoder_create(rate as i32, 1, OPUS_APPLICATION_VOIP, &mut code) };
        let encoder = Encoder(NonNull::new(st).ok_or_else(|| error(code))?);
        // SAFETY: each request takes one opus_int32 argument.
        unsafe {
            check(opus_encoder_ctl(encoder.0.as_ptr(), OPUS_SET_BITRATE_REQUEST, bitrate))?;
            if let Some(loss) = loss_percent {
                check(opus_encoder_ctl(encoder.0.as_ptr(), OPUS_SET_INBAND_FEC_REQUEST, 1i32))?;
                check(opus_encoder_ctl(encoder.0.as_ptr(), OPUS_SET_PACKET_LOSS_PERC_REQUEST, loss))?;
            }
        }
        Ok(encoder)
    }

    pub fn encode(&mut self, pcm: &[i16], packet: &mut [u8]) -> Result<usize, String> {
        // SAFETY: `pcm` holds frame_size mono samples, `packet` its stated length.
        let size = unsafe {
            opus_encode(self.0.as_ptr(), pcm.as_ptr(), pcm.len() as c_int, packet.as_mut_ptr(), packet.len().min(i32::MAX as usize) as i32)
        };
        check(size)?;
        Ok(size as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_encoder_create, destroyed once.
        unsafe { opus_encoder_destroy(self.0.as_ptr()) }
    }
}

pub struct Decoder {
    st: NonNull<c_void>,
    channels: usize,
}

// SAFETY: as for Encoder.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(rate: u32, channels: usize) -> Result<Self, String> {
        let mut code = 0;
        // SAFETY: valid arguments; the result is checked for NULL.
        let st = unsafe { opus_decoder_create(rate as i32, channels as c_int, &mut code) };
        Ok(Decoder { st: NonNull::new(st).ok_or_else(|| error(code))?, channels })
    }

    /// `frame` samples per channel of room in `out`; an empty packet = lost.
    pub fn decode(&mut self, packet: &[u8], out: &mut [f32], frame: usize, fec: bool) -> Result<usize, String> {
        let frame = frame.min(out.len() / self.channels);
        let data = if packet.is_empty() { std::ptr::null() } else { packet.as_ptr() };
        // SAFETY: `out` holds at least frame * channels floats; `data` is
        // NULL (lost) or `packet.len()` bytes.
        let size = unsafe {
            opus_decode_float(self.st.as_ptr(), data, packet.len().min(i32::MAX as usize) as i32, out.as_mut_ptr(), frame as c_int, c_int::from(fec))
        };
        check(size)?;
        Ok(size as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_decoder_create, destroyed once.
        unsafe { opus_decoder_destroy(self.st.as_ptr()) }
    }
}
