package com.exponential.app.data.call

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import android.os.Build
import android.util.Log
import androidx.annotation.RequiresApi
import com.exponential.app.domain.VoiceCall
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.ArrayDeque
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.min
import kotlin.math.sqrt

/**
 * NEW-69: a phone call's audio (`fixtures/voice-call.json` AUDIO). Uplink:
 * AudioRecord (VOICE_COMMUNICATION, the OS echo canceller on its session, a
 * noise suppressor where there is one) at [VoiceCall.Constants.UP_RATE] Hz
 * mono, every 20 ms frame into MediaCodec's Opus encoder, each packet out as
 * one `up` frame. Downlink: the host's `down` packets through MediaCodec's
 * Opus decoder into a [VoiceCall.Constants.JITTER_MS] ms jitter buffer, played
 * by an AudioTrack (USAGE_VOICE_COMMUNICATION). MediaCodec exposes no switch
 * for Opus in-band FEC, so the uplink goes without it (the host's decoder
 * conceals a lost packet from the next one when FEC is present, and plays
 * the gap otherwise). Opus encoding needs API 29 ([isSupported]).
 *
 * Every failure reaches [onFailure] ONCE with a `copy.failed` code
 * (`no_microphone` when the microphone cannot open, else `dropped`). The
 * call holds transient audio focus: focus refused at start, or lost (an
 * incoming phone call, another app's call), drops the call. [release] frees
 * everything without blocking its caller and is safe to call twice; it puts
 * the routing back only once the capture and playback threads have exited
 * (#NEW-89), never under a recorder or track that is still running.
 */
internal class VoiceCallAudio(
    private val context: Context,
    /** One whole uplink frame (`[1, seq hi, seq lo] + packet`), off the capture thread. */
    private val onUplink: (ByteArray) -> Unit,
    private val onFailure: (code: String) -> Unit,
) {
    /** Capture feeds the encoder only while connected. */
    @Volatile var sending: Boolean = false

    /** Muted, capture feeds the encoder silence ([VoiceCall.Uplink]): the relay's idle keep-alive. */
    @Volatile var muted: Boolean = false

    /** The microphone's level (0…1) of the last frame. */
    @Volatile var micLevel: Float = 0f
        private set

    /** The reply's level (0…1) of the frame playing now. */
    @Volatile var playLevel: Float = 0f
        private set

    private val running = AtomicBoolean(false)
    private val failed = AtomicBoolean(false)
    private val released = AtomicBoolean(false)
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focus: AudioFocusRequest? = null

    private var captureThread: Thread? = null
    private var decodeThread: Thread? = null
    private var playThread: Thread? = null
    private val downlink = LinkedBlockingQueue<ByteArray>(DOWNLINK_QUEUE)
    private val jitter = JitterBuffer(prebuffer = VoiceCall.Constants.JITTER_MS / VoiceCall.Constants.FRAME_MS)

    /** The decoder's output rate (the AudioTrack follows it). */
    @Volatile private var playRate: Int = VoiceCall.Constants.DOWN_RATE

    companion object {
        private const val TAG = "VoiceCallAudio"
        private const val MIME = MediaFormat.MIMETYPE_AUDIO_OPUS
        private const val UP_BITRATE = VoiceCall.Constants.UP_KBPS * 1000
        private const val UP_FRAME = VoiceCall.Constants.UP_RATE * VoiceCall.Constants.FRAME_MS / 1000
        private const val DOWNLINK_QUEUE = 64
        private const val CODEC_TIMEOUT_US = 10_000L
        /** How long [release] waits for each audio thread before restoring the routing. */
        private const val JOIN_MS = 500L
        /** Opus' standard seek pre-roll (80 ms) for the decoder's csd-2. */
        private const val SEEK_PREROLL_NS = 80_000_000L

        /**
         * A call can capture here: a microphone, API 29 (MediaCodec's Opus
         * encoder) and an Opus encoder + decoder on the device.
         */
        fun isSupported(context: Context): Boolean {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return false
            if (!context.packageManager.hasSystemFeature(PackageManager.FEATURE_MICROPHONE)) return false
            return runCatching {
                val list = MediaCodecList(MediaCodecList.REGULAR_CODECS)
                list.findEncoderForFormat(upFormat()) != null && list.findDecoderForFormat(downFormat()) != null
            }.getOrDefault(false)
        }

        private fun upFormat(): MediaFormat =
            MediaFormat.createAudioFormat(MIME, VoiceCall.Constants.UP_RATE, 1).apply {
                setInteger(MediaFormat.KEY_BIT_RATE, UP_BITRATE)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, UP_FRAME * 2 * 4)
            }

        private fun downFormat(): MediaFormat =
            MediaFormat.createAudioFormat(MIME, VoiceCall.Constants.DOWN_RATE, 1).apply {
                setByteBuffer("csd-0", ByteBuffer.wrap(opusHead(VoiceCall.Constants.DOWN_RATE)))
                setByteBuffer("csd-1", ByteBuffer.wrap(le64(0L)))
                setByteBuffer("csd-2", ByteBuffer.wrap(le64(SEEK_PREROLL_NS)))
            }

        /** RFC 7845 §5.1 identification header: mono, no pre-skip, family 0. */
        internal fun opusHead(rate: Int): ByteArray =
            ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN).apply {
                put("OpusHead".toByteArray(Charsets.US_ASCII))
                put(1) // version
                put(1) // channels
                putShort(0) // pre-skip
                putInt(rate)
                putShort(0) // output gain
                put(0) // mapping family
            }.array()

        /** The routing before the first of back-to-back calls (`mode` to `speakerphone`). */
        private val routing = RoutingHandoff<Pair<Int, Boolean>>()

        /**
         * Waits for every audio thread to exit, then [restore]s: the routing
         * never changes under a recorder or track that is still running.
         */
        internal fun joinThenRestore(threads: List<Thread>, timeoutMs: Long, restore: () -> Unit) {
            for (thread in threads) runCatching { thread.join(timeoutMs) }
            restore()
        }

        private fun le64(value: Long): ByteArray =
            ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

        /** A frame's RMS as a 0…1 level (speech sits around 0.05–0.2 RMS). */
        internal fun level(samples: ShortArray, count: Int): Float {
            if (count <= 0) return 0f
            var sum = 0.0
            for (i in 0 until count) {
                val s = samples[i] / 32768.0
                sum += s * s
            }
            return min(1f, (sqrt(sum / count) * 5.0).toFloat())
        }
    }

    /** Route for a call (communication mode, the speaker unless a headset is on), open the mic and the player. */
    @RequiresApi(Build.VERSION_CODES.Q)
    fun start() {
        if (!running.compareAndSet(false, true)) return
        if (!routeForCall()) return fail("dropped")
        captureThread = Thread({ capture() }, "call-capture").apply { start() }
        decodeThread = Thread({ decode() }, "call-decode").apply { start() }
        playThread = Thread({ play() }, "call-play").apply { start() }
    }

    /** One `down` packet (the wire header stripped). */
    fun pushDownlink(packet: ByteArray) {
        if (!running.get()) return
        // Realtime: a full queue drops the oldest packet rather than lag.
        if (!downlink.offer(packet)) {
            downlink.poll()
            downlink.offer(packet)
        }
    }

    /** Barge-in (`call_event` speechStarted): forget the buffered reply. */
    fun flushPlayback() {
        downlink.clear()
        jitter.flush()
    }

    fun release() {
        if (!released.compareAndSet(false, true)) return
        running.set(false)
        sending = false
        downlink.clear()
        jitter.flush()
        val threads = listOfNotNull(captureThread, decodeThread, playThread)
        for (thread in threads) thread.interrupt()
        micLevel = 0f
        playLevel = 0f
        // The threads free their recorder, codecs and track as they exit;
        // the routing goes back after them, and waiting for them never
        // blocks the caller (often the main thread).
        Thread({ joinThenRestore(threads, JOIN_MS) { restoreRouting() } }, "call-release").start()
    }

    private fun fail(code: String, cause: Throwable? = null) {
        if (!running.get() || !failed.compareAndSet(false, true)) return
        Log.w(TAG, "call audio failed ($code)", cause)
        onFailure(code)
    }

    // --- routing ---------------------------------------------------------

    /** False = audio focus was refused (another call holds it). */
    @Suppress("DEPRECATION")
    private fun routeForCall(): Boolean {
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(attributes)
            .setOnAudioFocusChangeListener { change ->
                // An incoming phone call (or another app's call) takes the
                // audio: the agent must never talk over it.
                if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                    fail("dropped")
                }
            }
            .build()
        val granted = runCatching { audioManager.requestAudioFocus(request) }.getOrNull()
        if (granted != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            Log.w(TAG, "call audio focus refused ($granted)")
            return false
        }
        focus = request
        runCatching {
            routing.claim(this) { audioManager.mode to audioManager.isSpeakerphoneOn }
            audioManager.mode = AudioManager.MODE_IN_COMMUNICATION
            // The call is a full screen held in front of the face: the
            // loudspeaker, unless a headset (wired, USB or Bluetooth) is on.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val devices = audioManager.availableCommunicationDevices
                val headset = devices.firstOrNull { it.type in HEADSET_TYPES }
                val target = headset ?: devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                target?.let { audioManager.setCommunicationDevice(it) }
            } else {
                val headset = audioManager.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any { it.type in HEADSET_TYPES }
                audioManager.isSpeakerphoneOn = !headset
            }
        }.onFailure { Log.w(TAG, "call routing failed", it) }
        return true
    }

    @Suppress("DEPRECATION")
    private fun restoreRouting() {
        // Never routed (focus refused): another app's call owns the mode.
        val request = focus ?: return
        runCatching {
            // A call started since then routes now: leave its routing alone.
            routing.restore(this)?.let { (previousMode, previousSpeaker) ->
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    audioManager.clearCommunicationDevice()
                } else {
                    audioManager.isSpeakerphoneOn = previousSpeaker
                }
                audioManager.mode = previousMode
            }
            audioManager.abandonAudioFocusRequest(request)
            focus = null
        }
    }

    // --- uplink ----------------------------------------------------------

    @SuppressLint("MissingPermission") // The controller checks RECORD_AUDIO before a call starts.
    private fun capture() {
        val rate = VoiceCall.Constants.UP_RATE
        val minBuffer = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (minBuffer <= 0) return fail("no_microphone")
        var record: AudioRecord? = null
        var echo: AcousticEchoCanceler? = null
        var noise: NoiseSuppressor? = null
        var encoder: MediaCodec? = null
        try {
            record = try {
                AudioRecord(
                    MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                    rate,
                    AudioFormat.CHANNEL_IN_MONO,
                    AudioFormat.ENCODING_PCM_16BIT,
                    maxOf(minBuffer, UP_FRAME * 2 * 4),
                )
            } catch (t: Throwable) {
                return fail("no_microphone", t)
            }
            if (record.state != AudioRecord.STATE_INITIALIZED) return fail("no_microphone")
            if (AcousticEchoCanceler.isAvailable()) {
                echo = runCatching { AcousticEchoCanceler.create(record.audioSessionId)?.apply { enabled = true } }.getOrNull()
            }
            if (NoiseSuppressor.isAvailable()) {
                noise = runCatching { NoiseSuppressor.create(record.audioSessionId)?.apply { enabled = true } }.getOrNull()
            }
            encoder = MediaCodec.createEncoderByType(MIME).apply {
                configure(upFormat(), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                start()
            }
            try {
                record.startRecording()
            } catch (t: Throwable) {
                return fail("no_microphone", t)
            }
            if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) return fail("no_microphone")
            val frame = ShortArray(UP_FRAME)
            val info = MediaCodec.BufferInfo()
            var ptsUs = 0L
            var seq = 0
            while (running.get()) {
                var filled = 0
                while (filled < UP_FRAME && running.get()) {
                    val read = record.read(frame, filled, UP_FRAME - filled)
                    if (read < 0) return fail("dropped", IllegalStateException("AudioRecord.read = $read"))
                    filled += read
                }
                if (!running.get()) break
                if (!sending) {
                    micLevel = 0f
                    continue
                }
                val muted = muted
                micLevel = if (muted) 0f else level(frame, filled)
                val samples = VoiceCall.Uplink.samples(frame, filled, muted)
                val input = encoder.dequeueInputBuffer(CODEC_TIMEOUT_US)
                if (input >= 0) {
                    val buffer = encoder.getInputBuffer(input) ?: continue
                    buffer.clear()
                    buffer.order(ByteOrder.nativeOrder()).asShortBuffer().put(samples, 0, filled)
                    encoder.queueInputBuffer(input, 0, filled * 2, ptsUs, 0)
                    ptsUs += VoiceCall.Constants.FRAME_MS * 1000L
                }
                while (true) {
                    val output = encoder.dequeueOutputBuffer(info, 0)
                    if (output < 0) break
                    val config = info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0
                    if (!config && info.size > 0) {
                        val buffer = encoder.getOutputBuffer(output)
                        if (buffer != null) {
                            val packet = ByteArray(info.size)
                            buffer.position(info.offset)
                            buffer.get(packet)
                            // A packet above the cap would be dropped by the relay anyway.
                            if (packet.size + 3 <= VoiceCall.Wire.MAX_FRAME_BYTES) {
                                onUplink(VoiceCall.Wire.frame(VoiceCall.Wire.KIND_UP, seq, packet))
                                seq = (seq + 1) and 0xffff
                            }
                        }
                    }
                    encoder.releaseOutputBuffer(output, false)
                }
            }
        } catch (_: InterruptedException) {
            // release()
        } catch (t: Throwable) {
            fail("dropped", t)
        } finally {
            runCatching { record?.stop() }
            runCatching { record?.release() }
            runCatching { echo?.release() }
            runCatching { noise?.release() }
            runCatching { encoder?.stop() }
            runCatching { encoder?.release() }
            micLevel = 0f
        }
    }

    // --- downlink --------------------------------------------------------

    private fun decode() {
        var decoder: MediaCodec? = null
        try {
            decoder = MediaCodec.createDecoderByType(MIME).apply {
                configure(downFormat(), null, null, 0)
                start()
            }
            val info = MediaCodec.BufferInfo()
            var ptsUs = 0L
            var channels = 1
            while (running.get()) {
                val packet = downlink.poll(VoiceCall.Constants.FRAME_MS.toLong(), TimeUnit.MILLISECONDS)
                if (packet != null) {
                    val input = decoder.dequeueInputBuffer(CODEC_TIMEOUT_US)
                    if (input >= 0) {
                        val buffer = decoder.getInputBuffer(input)
                        if (buffer != null) {
                            buffer.clear()
                            buffer.put(packet)
                            decoder.queueInputBuffer(input, 0, packet.size, ptsUs, 0)
                            ptsUs += VoiceCall.Constants.FRAME_MS * 1000L
                        }
                    }
                }
                while (true) {
                    val output = decoder.dequeueOutputBuffer(info, 0)
                    if (output == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                        val format = decoder.outputFormat
                        // The platform decoder may answer at its own rate (48 kHz): the player follows.
                        playRate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT).coerceAtLeast(1)
                        continue
                    }
                    if (output < 0) break
                    if (info.size > 0) {
                        val buffer = decoder.getOutputBuffer(output)
                        if (buffer != null) {
                            buffer.position(info.offset)
                            buffer.limit(info.offset + info.size)
                            val shorts = buffer.order(ByteOrder.nativeOrder()).asShortBuffer()
                            val all = ShortArray(shorts.remaining())
                            shorts.get(all)
                            jitter.push(if (channels == 1) all else ShortArray(all.size / channels) { all[it * channels] })
                        }
                    }
                    decoder.releaseOutputBuffer(output, false)
                }
            }
        } catch (_: InterruptedException) {
            // release()
        } catch (t: Throwable) {
            fail("dropped", t)
        } finally {
            runCatching { decoder?.stop() }
            runCatching { decoder?.release() }
        }
    }

    private fun play() {
        var track: AudioTrack? = null
        try {
            while (running.get()) {
                val rate = playRate
                if (track == null || track.sampleRate != rate) {
                    track?.let { runCatching { it.stop() }; it.release() }
                    track = buildTrack(rate).also { it.play() }
                }
                if (jitter.takeFlush()) {
                    track.pause()
                    track.flush()
                    track.play()
                }
                val frame = jitter.take()
                if (frame == null) {
                    playLevel = 0f
                    // Silence keeps the track running at real time while it prebuffers.
                    track.write(ShortArray(rate * VoiceCall.Constants.FRAME_MS / 1000), 0, rate * VoiceCall.Constants.FRAME_MS / 1000)
                    continue
                }
                playLevel = level(frame, frame.size)
                val written = track.write(frame, 0, frame.size)
                if (written < 0) return fail("dropped", IllegalStateException("AudioTrack.write = $written"))
            }
        } catch (_: InterruptedException) {
            // release()
        } catch (t: Throwable) {
            fail("dropped", t)
        } finally {
            runCatching { track?.stop() }
            runCatching { track?.release() }
            playLevel = 0f
        }
    }

    private fun buildTrack(rate: Int): AudioTrack {
        val minBuffer = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val track = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build(),
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setSampleRate(rate)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                    .build(),
            )
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setBufferSizeInBytes(maxOf(minBuffer, rate * VoiceCall.Constants.FRAME_MS / 1000 * 2 * 2))
            .build()
        if (track.state != AudioTrack.STATE_INITIALIZED) {
            track.release()
            throw IllegalStateException("AudioTrack did not initialize at $rate Hz")
        }
        return track
    }
}

private val HEADSET_TYPES = setOf(
    AudioDeviceInfo.TYPE_WIRED_HEADSET,
    AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
    AudioDeviceInfo.TYPE_USB_HEADSET,
    AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
    AudioDeviceInfo.TYPE_BLE_HEADSET,
)

/**
 * The downlink's jitter buffer: playback starts once [prebuffer] decoded
 * frames are queued (`constants.jitterMs` / `frameMs`), re-prebuffers on
 * every underrun, and [flush] (barge-in) empties it at once. A queue grown
 * past [cap] frames sheds its oldest: a late reply frame is worth nothing.
 */
internal class JitterBuffer(private val prebuffer: Int, private val cap: Int = prebuffer * 6) {
    private val frames = ArrayDeque<ShortArray>()
    private var prebuffering = true
    private var flushRequested = false

    @Synchronized fun push(frame: ShortArray) {
        if (frame.isEmpty()) return
        frames.addLast(frame)
        while (frames.size > cap) frames.removeFirst()
    }

    /** The next frame to play; null while it (re-)prebuffers. */
    @Synchronized fun take(): ShortArray? {
        if (prebuffering) {
            if (frames.size < prebuffer) return null
            prebuffering = false
        }
        val frame = frames.pollFirst()
        if (frame == null) prebuffering = true
        return frame
    }

    @Synchronized fun flush() {
        frames.clear()
        prebuffering = true
        flushRequested = true
    }

    /** Whether a flush asked the player to drop what its track still holds. */
    @Synchronized fun takeFlush(): Boolean {
        val asked = flushRequested
        flushRequested = false
        return asked
    }
}

/**
 * #NEW-89: the call routing's saved state across back-to-back calls. A call
 * restores the routing only after its audio threads exit, so the next call
 * may route first: it keeps the state saved before the FIRST call, and the
 * previous call's late [restore] gets nothing (it would undo the new
 * call's routing).
 */
internal class RoutingHandoff<S : Any> {
    private var owner: Any? = null
    private var saved: S? = null

    /** [owner] routes now; [current] is read only when no call holds the routing. */
    @Synchronized fun claim(owner: Any, current: () -> S) {
        if (this.owner == null) saved = current()
        this.owner = owner
    }

    /** The state to put back, when [owner] still holds the routing; else null. */
    @Synchronized fun restore(owner: Any): S? {
        if (this.owner !== owner) return null
        this.owner = null
        return saved.also { saved = null }
    }
}
