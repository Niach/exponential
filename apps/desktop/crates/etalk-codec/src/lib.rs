//! NEW-88: the call's Opus (`fixtures/voice-call.json` AUDIO), one seam for
//! the call host (`steer::call_host`), the latency example and the fuzz
//! targets (`apps/desktop/fuzz`). [`Encoder`] and [`Decoder`] run on
//! `opus-rs` by default and on the system libopus with the `libopus`
//! feature; [`Uplink`] is the call host's receive path (a caller's packets
//! by sequence number, one lost packet recovered from the next one's FEC).

#[cfg(feature = "libopus")]
mod libopus;

/// Which Opus implementation this build codes with.
pub const BACKEND: &str = if cfg!(feature = "libopus") {
    "libopus"
} else if cfg!(feature = "opus-rs") {
    "opus-rs"
} else {
    "none"
};

#[cfg(not(any(feature = "libopus", feature = "opus-rs")))]
compile_error!("etalk-codec needs an Opus backend: the `opus-rs` (default) or the `libopus` feature");

/// voice-call.json `constants.frameMs`.
pub const FRAME_MS: u32 = 20;

/// Opus only codes 8, 12, 16, 24 and 48 kHz; anything else is a bug upstream.
pub fn opus_rate(rate: u32) -> Option<u32> {
    [8_000, 12_000, 16_000, 24_000, 48_000].contains(&rate).then_some(rate)
}

/// Runs one `opus-rs` call, a panic turned into an error: the decoder
/// codes packets off the network, and a panic would take the call host's
/// task down with it (opus-rs 0.1.40's encoder divides by zero with in-band
/// FEC at 24/48 kHz and 20-28 kbps). The caller rebuilds the codec after a
/// panic. Fuzzing (`--cfg fuzzing`) sees the raw panic.
#[cfg(not(feature = "libopus"))]
fn guarded<T>(call: impl FnOnce() -> Result<T, &'static str>) -> Result<T, Guarded> {
    #[cfg(fuzzing)]
    return call().map_err(|err| Guarded::Failed(err.to_string()));
    #[cfg(not(fuzzing))]
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
        Ok(result) => result.map_err(|err| Guarded::Failed(err.to_string())),
        Err(_) => Err(Guarded::Panicked),
    }
}

#[cfg(not(feature = "libopus"))]
enum Guarded {
    Failed(String),
    #[cfg_attr(fuzzing, allow(dead_code))]
    Panicked,
}

/// A mono VoIP encoder.
pub struct Encoder {
    #[cfg(feature = "libopus")]
    inner: libopus::Encoder,
    #[cfg(not(feature = "libopus"))]
    inner: opus_rs::OpusEncoder,
    #[cfg(not(feature = "libopus"))]
    config: (u32, i32, Option<i32>),
}

impl Encoder {
    /// `loss_percent` = Some turns in-band FEC on, tuned for that loss.
    pub fn new(rate: u32, bitrate: i32, loss_percent: Option<i32>) -> Result<Self, String> {
        let rate = opus_rate(rate).ok_or_else(|| format!("opus cannot code {rate} Hz"))?;
        #[cfg(feature = "libopus")]
        let inner = libopus::Encoder::new(rate, bitrate, loss_percent)?;
        #[cfg(not(feature = "libopus"))]
        let inner = Self::opus_rs(rate, bitrate, loss_percent)?;
        Ok(Encoder {
            inner,
            #[cfg(not(feature = "libopus"))]
            config: (rate, bitrate, loss_percent),
        })
    }

    #[cfg(not(feature = "libopus"))]
    fn opus_rs(rate: u32, bitrate: i32, loss_percent: Option<i32>) -> Result<opus_rs::OpusEncoder, String> {
        let mut opus = opus_rs::OpusEncoder::new(rate as i32, 1, opus_rs::Application::Voip).map_err(str::to_string)?;
        opus.bitrate_bps = bitrate;
        if let Some(loss) = loss_percent {
            opus.use_inband_fec = true;
            opus.packet_loss_perc = loss;
        }
        Ok(opus)
    }

    /// One frame (`pcm.len()` samples, a valid Opus duration) into `packet`;
    /// the packet's size.
    pub fn encode(&mut self, pcm: &[i16], packet: &mut [u8]) -> Result<usize, String> {
        #[cfg(feature = "libopus")]
        return self.inner.encode(pcm, packet);
        #[cfg(not(feature = "libopus"))]
        return match guarded(|| self.inner.encode_i16(pcm, pcm.len(), packet)) {
            Ok(size) => Ok(size),
            Err(Guarded::Failed(err)) => Err(err),
            Err(Guarded::Panicked) => {
                let (rate, bitrate, loss_percent) = self.config;
                self.inner = Self::opus_rs(rate, bitrate, loss_percent)?;
                Err("opus-rs panicked encoding the frame".to_string())
            }
        };
    }
}

/// A decoder of `channels` (1 or 2) interleaved channels.
pub struct Decoder {
    #[cfg(not(feature = "libopus"))]
    rate: u32,
    channels: usize,
    #[cfg(feature = "libopus")]
    inner: libopus::Decoder,
    #[cfg(not(feature = "libopus"))]
    inner: opus_rs::OpusDecoder,
}

impl Decoder {
    pub fn new(rate: u32, channels: usize) -> Result<Self, String> {
        let rate = opus_rate(rate).ok_or_else(|| format!("opus cannot decode at {rate} Hz"))?;
        if !(1..=2).contains(&channels) {
            return Err(format!("opus cannot decode {channels} channels"));
        }
        #[cfg(feature = "libopus")]
        let inner = libopus::Decoder::new(rate, channels)?;
        #[cfg(not(feature = "libopus"))]
        let inner = opus_rs::OpusDecoder::new(rate as i32, channels).map_err(str::to_string)?;
        Ok(Decoder {
            #[cfg(not(feature = "libopus"))]
            rate,
            channels,
            inner,
        })
    }

    /// `packet` into `out` (room for `out.len() / channels` samples per
    /// channel); the samples per channel decoded. An empty packet conceals a
    /// lost one.
    pub fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, String> {
        let frame = out.len() / self.channels;
        #[cfg(feature = "libopus")]
        return self.inner.decode(packet, out, frame, false);
        #[cfg(not(feature = "libopus"))]
        return self.recover(|opus| opus.decode(packet, frame, out));
    }

    /// The packet BEFORE `packet` (lost), from `packet`'s in-band FEC, into
    /// `out` (exactly the lost frame's duration). `packet` itself is decoded
    /// by [`Decoder::decode`] next.
    pub fn decode_fec(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, String> {
        let frame = out.len() / self.channels;
        #[cfg(feature = "libopus")]
        return self.inner.decode(packet, out, frame, true);
        #[cfg(not(feature = "libopus"))]
        return self.recover(|opus| opus.decode_fec(packet, frame, out));
    }

    /// One guarded decode; a panic leaves a fresh decoder behind.
    #[cfg(not(feature = "libopus"))]
    fn recover(&mut self, call: impl FnOnce(&mut opus_rs::OpusDecoder) -> Result<usize, &'static str>) -> Result<usize, String> {
        match guarded(|| call(&mut self.inner)) {
            Ok(size) => Ok(size),
            Err(Guarded::Failed(err)) => Err(err),
            Err(Guarded::Panicked) => {
                self.inner = opus_rs::OpusDecoder::new(self.rate as i32, self.channels).map_err(str::to_string)?;
                Err("opus-rs panicked decoding the packet".to_string())
            }
        }
    }
}

/// f32 PCM in -1..1 as i16 samples.
fn to_i16(samples: &[f32]) -> impl Iterator<Item = i16> + '_ {
    samples.iter().map(|sample| (sample.clamp(-1., 1.) * 32767.) as i16)
}

/// The call host's receive path: a caller's mono packets of [`FRAME_MS`] at
/// `rate`; a lost sequence number is concealed with FEC from the next packet.
pub struct Uplink {
    decoder: Decoder,
    frame: usize,
    next: Option<u16>,
}

impl Uplink {
    pub fn new(rate: u32) -> Result<Self, String> {
        Ok(Uplink { decoder: Decoder::new(rate, 1)?, frame: (rate * FRAME_MS / 1000) as usize, next: None })
    }

    /// The PCM of packet `seq` (and of the one lost before it), empty when
    /// it does not decode.
    pub fn push(&mut self, seq: u16, packet: &[u8]) -> Vec<i16> {
        let mut out = Vec::new();
        // One lost packet: recover it from this one's in-band FEC.
        if self.next.is_some_and(|next| seq == next.wrapping_add(1)) {
            let mut recovered = vec![0f32; self.frame];
            if let Ok(size) = self.decoder.decode_fec(packet, &mut recovered) {
                out.extend(to_i16(&recovered[..size.min(self.frame)]));
            }
        }
        let mut pcm = vec![0f32; self.frame];
        match self.decoder.decode(packet, &mut pcm) {
            Ok(size) => out.extend(to_i16(&pcm[..size.min(self.frame)])),
            Err(err) => log::debug!("call codec: opus decode failed: {err}"),
        }
        self.next = Some(seq.wrapping_add(1));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, len: usize) -> Vec<i16> {
        (0..len).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16).collect()
    }

    #[test]
    fn a_frame_round_trips() {
        let mut encoder = Encoder::new(24_000, 32_000, None).unwrap();
        let mut decoder = Decoder::new(24_000, 1).unwrap();
        let mut packet = [0u8; 1500];
        let mut out = vec![0f32; 480];
        let mut energy = 0.0;
        for _ in 0..10 {
            let size = encoder.encode(&tone(24_000, 480), &mut packet).unwrap();
            assert!(size > 1 && size < 1500);
            assert_eq!(decoder.decode(&packet[..size], &mut out).unwrap(), 480);
            energy = out.iter().map(|sample| sample * sample).sum::<f32>();
        }
        assert!(energy > 1.0, "the tone survives coding ({energy})");
    }

    #[test]
    fn a_lost_packet_is_recovered_from_fec() {
        let mut encoder = Encoder::new(16_000, 24_000, Some(10)).unwrap();
        let mut uplink = Uplink::new(16_000).unwrap();
        let mut packet = [0u8; 1500];
        let mut packets = Vec::new();
        for _ in 0..6 {
            let size = encoder.encode(&tone(16_000, 320), &mut packet).unwrap();
            packets.push(packet[..size].to_vec());
        }
        for seq in 0..4u16 {
            assert_eq!(uplink.push(seq, &packets[seq as usize]).len(), 320);
        }
        // 4 is lost: 5 carries both frames.
        assert_eq!(uplink.push(5, &packets[5]).len(), 640);
    }

    #[test]
    fn garbage_and_empty_packets_never_panic() {
        let mut uplink = Uplink::new(16_000).unwrap();
        for (seq, packet) in [&[][..], &[0xff], &[0xfc, 0xff, 0xff, 0xff], &[3, 0, 0, 0, 0, 0]].iter().enumerate() {
            let pcm = uplink.push(seq as u16 * 2, packet);
            assert!(pcm.len() <= 640);
        }
    }

    /// opus-rs 0.1.40 panics on frame 0 here (rate.rs: divide by zero); the
    /// guard turns it into an error and the encoder keeps working. libopus
    /// codes it.
    #[test]
    fn an_encoder_panic_is_an_error() {
        let mut encoder = Encoder::new(24_000, 24_000, Some(10)).unwrap();
        let mut packet = [0u8; 1500];
        for _ in 0..3 {
            match encoder.encode(&tone(24_000, 480), &mut packet) {
                Ok(size) => assert!(size > 0),
                Err(err) => assert_eq!(BACKEND, "opus-rs", "{err}"),
            }
        }
    }

    #[test]
    fn only_opus_rates_and_channels() {
        assert!(Encoder::new(22_050, 32_000, None).is_err());
        assert!(Encoder::new(48_000, 32_000, None).is_ok());
        assert!(Decoder::new(16_000, 3).is_err());
    }
}
