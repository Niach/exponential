//! WASAPI capture clients: process loopback of one process tree
//! (`ActivateAudioInterfaceAsync` on `VAD\Process_Loopback`, build 20348+)
//! and plain loopback of the default render device. Shared mode with
//! AUTOCONVERTPCM so the engine hands us mono f32 at 16 kHz where it can;
//! [`pcm::Converter`] takes any other format the client settles on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use windows::core::{implement, IUnknown, Interface, HRESULT};
use windows::Win32::Media::Audio::{
    eConsole, eRender, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient,
    IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, BLOB, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VT_BLOB;

use super::super::SAMPLE_RATE;
use super::pcm;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// The client's buffer: 200 ms (in 100 ns units).
const BUFFER_HNS: i64 = 2_000_000;

/// COM (multithreaded) for the current thread, released on drop.
pub(super) struct Com;

impl Com {
    pub(super) fn init() -> Result<Com, String> {
        // SAFETY: paired with CoUninitialize in Drop, on the same thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        hr.ok().map(|()| Com).map_err(|error| format!("COM: {error}"))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: this thread's successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

/// Signals the activation's completion (any thread: the object is agile).
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Completion {
    done: Mutex<Option<mpsc::Sender<()>>>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _operation: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        if let Some(done) = self.done.lock().ok().and_then(|mut done| done.take()) {
            let _ = done.send(());
        }
        Ok(())
    }
}

/// An IAudioClient that loops back `pid` and its whole process tree.
fn activate_process_loopback(pid: u32) -> Result<IAudioClient, String> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let mut variant = PROPVARIANT::default();
    // SAFETY: a VT_BLOB PROPVARIANT pointing at `params`, which outlives the
    // activation below; never PropVariantClear'ed (it owns nothing).
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BLOB;
        inner.Anonymous.blob = BLOB {
            cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: (&mut params as *mut AUDIOCLIENT_ACTIVATION_PARAMS).cast(),
        };
    }
    let (done, completed) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Completion { done: Mutex::new(Some(done)) }.into();
    // SAFETY: the documented process-loopback activation; `variant` and
    // `params` live until the completion has fired.
    let operation = unsafe {
        ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&variant), &handler)
    }
    .map_err(|error| format!("process loopback activation: {error}"))?;
    completed
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "process loopback activation timed out".to_string())?;
    let mut result = HRESULT(0);
    let mut activated: Option<IUnknown> = None;
    // SAFETY: the operation has completed.
    unsafe { operation.GetActivateResult(&mut result, &mut activated) }
        .map_err(|error| format!("process loopback result: {error}"))?;
    result.ok().map_err(|error| format!("process loopback: {error}"))?;
    activated
        .ok_or("process loopback: no interface")?
        .cast::<IAudioClient>()
        .map_err(|error| format!("process loopback client: {error}"))
}

/// The default render device's IAudioClient (system loopback).
fn activate_default_render() -> Result<IAudioClient, String> {
    // SAFETY: COM calls on interfaces this function owns.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|error| format!("devices: {error}"))?;
        let device =
            enumerator.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|error| format!("no output device: {error}"))?;
        device.Activate::<IAudioClient>(CLSCTX_ALL, None).map_err(|error| format!("output device: {error}"))
    }
}

/// Whether a default render device exists (the `system` capability).
pub(super) fn has_default_render() -> bool {
    activate_default_render().is_ok()
}

/// How the client's samples are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub float: bool,
    pub bits: u16,
    pub channels: u16,
    pub rate: u32,
}

impl Layout {
    fn wave(&self) -> WAVEFORMATEX {
        let block = self.channels * (self.bits / 8);
        WAVEFORMATEX {
            wFormatTag: if self.float { WAVE_FORMAT_IEEE_FLOAT } else { WAVE_FORMAT_PCM },
            nChannels: self.channels,
            nSamplesPerSec: self.rate,
            nAvgBytesPerSec: self.rate * block as u32,
            nBlockAlign: block,
            wBitsPerSample: self.bits,
            cbSize: 0,
        }
    }

    fn of(format: &WAVEFORMATEX) -> Option<Layout> {
        let float = match (format.wFormatTag, format.wBitsPerSample) {
            (WAVE_FORMAT_IEEE_FLOAT, 32) => true,
            (WAVE_FORMAT_PCM, 16) => false,
            // The mix format is WAVEFORMATEXTENSIBLE in practice: 32-bit
            // = float, 16-bit = PCM.
            (WAVE_FORMAT_EXTENSIBLE, 32) => true,
            (WAVE_FORMAT_EXTENSIBLE, 16) => false,
            _ => return None,
        };
        Some(Layout { float, bits: format.wBitsPerSample, channels: format.nChannels.max(1), rate: format.nSamplesPerSec })
    }

    fn frame_bytes(&self) -> usize {
        self.channels as usize * (self.bits as usize / 8)
    }

    fn decode(&self, bytes: &[u8]) -> Vec<f32> {
        if self.float { pcm::f32_le(bytes) } else { pcm::i16_le(bytes) }
    }
}

/// What we ask the engine for, in order: mono f32 at 16 kHz, then common
/// layouts it surely converts to.
const WANTED: [Layout; 3] = [
    Layout { float: true, bits: 32, channels: 1, rate: SAMPLE_RATE },
    Layout { float: true, bits: 32, channels: 2, rate: 48_000 },
    Layout { float: false, bits: 16, channels: 2, rate: 44_100 },
];

fn initialize(client: &IAudioClient, layout: Layout) -> windows::core::Result<()> {
    let flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    let wave = layout.wave();
    // SAFETY: a fully initialized WAVEFORMATEX that outlives the call.
    unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_HNS, 0, &wave, None) }
}

/// A started loopback client.
pub(super) struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    layout: Layout,
}

impl Loopback {
    /// Process loopback of `pid`'s tree (a fresh activation per format
    /// tried: a failed Initialize may leave the client unusable).
    pub(super) fn process(pid: u32) -> Result<Loopback, String> {
        let mut last = String::new();
        for layout in WANTED {
            let client = activate_process_loopback(pid)?;
            match initialize(&client, layout) {
                Ok(()) => return Loopback::start(client, layout),
                Err(error) => last = format!("{error}"),
            }
        }
        Err(format!("process loopback format: {last}"))
    }

    /// Loopback of the default render device (everything it plays).
    pub(super) fn system() -> Result<Loopback, String> {
        let client = activate_default_render()?;
        if initialize(&client, WANTED[0]).is_ok() {
            return Loopback::start(client, WANTED[0]);
        }
        // No AUTOCONVERTPCM support: the device's own mix format.
        let client = activate_default_render()?;
        // SAFETY: GetMixFormat's allocation is read once and freed here.
        let layout = unsafe {
            let format = client.GetMixFormat().map_err(|error| format!("mix format: {error}"))?;
            let layout = Layout::of(&*format);
            let result = layout.map(|_| {
                client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, BUFFER_HNS, 0, format, None)
            });
            CoTaskMemFree(Some(format as *const _));
            match result {
                Some(Ok(())) => layout,
                Some(Err(error)) => return Err(format!("loopback: {error}")),
                None => None,
            }
        };
        let layout = layout.ok_or("loopback: unsupported mix format")?;
        Loopback::start(client, layout)
    }

    fn start(client: IAudioClient, layout: Layout) -> Result<Loopback, String> {
        // SAFETY: an initialized client.
        unsafe {
            let capture: IAudioCaptureClient = client.GetService().map_err(|error| format!("capture client: {error}"))?;
            client.Start().map_err(|error| format!("start: {error}"))?;
            Ok(Loopback { client, capture, layout })
        }
    }

    /// Reads until `stop` (polling every 10 ms), handing mono 16 kHz
    /// samples to `sink`; `Err` when the client fails (device gone), `Ok`
    /// when stopped or `sink` returns false.
    pub(super) fn pump(&self, stop: &AtomicBool, mut sink: impl FnMut(Vec<f32>) -> bool) -> Result<(), String> {
        let mut converter = pcm::Converter::new(self.layout.rate, self.layout.channels as usize);
        let frame = self.layout.frame_bytes();
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
            loop {
                // SAFETY: the capture client's documented packet loop: every
                // GetBuffer is matched by a ReleaseBuffer of its frames.
                let samples = unsafe {
                    let frames = self.capture.GetNextPacketSize().map_err(|error| format!("capture: {error}"))?;
                    if frames == 0 {
                        break;
                    }
                    let mut data = std::ptr::null_mut();
                    let mut count = 0u32;
                    let mut flags = 0u32;
                    self.capture
                        .GetBuffer(&mut data, &mut count, &mut flags, None, None)
                        .map_err(|error| format!("capture: {error}"))?;
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                    let samples = if silent || data.is_null() {
                        vec![0.0; count as usize * self.layout.channels as usize]
                    } else {
                        self.layout.decode(std::slice::from_raw_parts(data, count as usize * frame))
                    };
                    self.capture.ReleaseBuffer(count).map_err(|error| format!("capture: {error}"))?;
                    samples
                };
                let mono = converter.convert(&samples);
                if !mono.is_empty() && !sink(mono) {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        // SAFETY: a started client.
        let _ = unsafe { self.client.Stop() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_formats_are_consistent() {
        for layout in WANTED {
            let wave = layout.wave();
            // Copies: WAVEFORMATEX is packed.
            let (align, average) = (wave.nBlockAlign, wave.nAvgBytesPerSec);
            assert_eq!(align as usize, layout.frame_bytes());
            assert_eq!(average, layout.rate * align as u32);
            assert_eq!(Layout::of(&wave), Some(layout));
        }
    }

    #[test]
    fn mix_formats_decode() {
        let mut wave = WANTED[1].wave();
        wave.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
        assert_eq!(Layout::of(&wave).map(|l| l.float), Some(true));
        wave.wBitsPerSample = 24;
        assert_eq!(Layout::of(&wave), None);
    }
}
