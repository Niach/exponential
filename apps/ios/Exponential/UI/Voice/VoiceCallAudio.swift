import AVFoundation
import ExpCore
import Foundation

// NEW-69 (voice-call.json TRANSPORT / AUDIO): a phone call's audio. The
// microphone runs through the OS echo canceller (`.voiceChat` + voice
// processing on the input node), is resampled to `upRate` mono Int16 and
// cut into `frameMs` frames, each ONE Opus packet (AVAudioConverter,
// ~`upKbps` kbps) sent as an `up` frame. Downlink `down` frames decode to
// `downRate` mono and play through a `jitterMs` jitter buffer on the same
// engine. AVAudioConverter exposes no Opus in-band FEC switch, so the
// uplink goes out without it (the host's decoder simply has nothing to
// recover a lost packet from).
//
// Every AVFoundation object lives on `queue`: the tap thread, the socket's
// receive thread and the main actor all hop onto it, so the class is
// `@unchecked Sendable` by confinement. The queue is ONE for every call, so
// a call's session teardown always runs before the next call's activation,
// and the main actor never waits on it (start resumes, stop is fire and
// forget).

enum VoiceCallAudioError: Error {
    /// No input route (voice-call.json `failed.no_microphone`).
    case noMicrophone
    /// The session or the engine refused (`failed.dropped`).
    case failed
}

final class VoiceCallAudio: @unchecked Sendable {
    /// One uplink wire frame (`[1, seq>>8, seq&0xff] + packet`).
    private let onUplink: @Sendable (Data) -> Void
    /// The level 0..1 of the microphone (`speaking` false) or of the reply
    /// being played (`speaking` true).
    private let onLevel: @Sendable (_ level: Double, _ speaking: Bool) -> Void
    /// The engine died mid-call (an interruption, a route it can't restart on).
    private let onFailure: @Sendable (_ code: String) -> Void

    private static let sharedQueue = DispatchQueue(label: "at.exponential.voice-call.audio", qos: .userInteractive)
    private var queue: DispatchQueue { Self.sharedQueue }
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()

    /// `upRate` mono Int16, what the encoder takes.
    private let upPcm = AVAudioFormat(
        commonFormat: .pcmFormatInt16, sampleRate: Double(VoiceCall.Constants.upRate), channels: 1, interleaved: true
    )!
    /// `downRate` mono float, what the player takes.
    private let downPcm = AVAudioFormat(
        commonFormat: .pcmFormatFloat32, sampleRate: Double(VoiceCall.Constants.downRate), channels: 1, interleaved: false
    )!
    private let upOpus = VoiceCallAudio.opusFormat(rate: VoiceCall.Constants.upRate)
    private let downOpus = VoiceCallAudio.opusFormat(rate: VoiceCall.Constants.downRate)

    private var resampler: AVAudioConverter?
    private var encoder: AVAudioConverter?
    private var decoder: AVAudioConverter?

    private var running = false
    private var uplink = VoiceCall.Uplink()
    private var seq: UInt16 = 0

    // The jitter buffer: `prebuffer` frames are held before playing starts,
    // and again after every underrun; then each frame is scheduled as it
    // arrives. A flush (barge-in) drops everything held and scheduled.
    private var held: [AVAudioPCMBuffer] = []
    private var scheduled = 0
    private var prebuffering = true
    private var playGeneration = 0
    private let prebuffer = max(1, VoiceCall.Constants.jitterMs / VoiceCall.Constants.frameMs)

    private var observers: [NSObjectProtocol] = []

    init(
        onUplink: @escaping @Sendable (Data) -> Void,
        onLevel: @escaping @Sendable (_ level: Double, _ speaking: Bool) -> Void,
        onFailure: @escaping @Sendable (_ code: String) -> Void
    ) {
        self.onUplink = onUplink
        self.onLevel = onLevel
        self.onFailure = onFailure
    }

    /// One mono Opus stream description at `rate` (`frameMs` per packet).
    private static func opusFormat(rate: Int) -> AVAudioFormat {
        var description = AudioStreamBasicDescription(
            mSampleRate: Double(rate),
            mFormatID: kAudioFormatOpus,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: UInt32(rate * VoiceCall.Constants.frameMs / 1000),
            mBytesPerFrame: 0,
            mChannelsPerFrame: 1,
            mBitsPerChannel: 0,
            mReserved: 0
        )
        return AVAudioFormat(streamDescription: &description)!
    }

    // MARK: - Lifecycle

    /// Opens the session and the engine and starts capturing.
    func start() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            queue.async {
                do {
                    try self.startOnQueue()
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    /// Stops everything and gives the audio session back. Idempotent, never
    /// waited on.
    func stop() {
        queue.async { self.stopOnQueue() }
    }

    private func startOnQueue() throws {
        guard !running else { return }
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetoothHFP])
            try? session.setPreferredIOBufferDuration(Double(VoiceCall.Constants.frameMs) / 1000)
            try session.setActive(true)
        } catch {
            throw VoiceCallAudioError.failed
        }
        guard session.isInputAvailable else {
            stopOnQueue()
            throw VoiceCallAudioError.noMicrophone
        }
        let input = engine.inputNode
        do {
            try input.setVoiceProcessingEnabled(true)
        } catch {
            stopOnQueue()
            throw VoiceCallAudioError.failed
        }
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.channelCount > 0, inFormat.sampleRate > 0 else {
            stopOnQueue()
            throw VoiceCallAudioError.noMicrophone
        }
        guard let resampler = AVAudioConverter(from: inFormat, to: upPcm),
              let encoder = AVAudioConverter(from: upPcm, to: upOpus),
              let decoder = AVAudioConverter(from: downOpus, to: downPcm)
        else {
            stopOnQueue()
            throw VoiceCallAudioError.failed
        }
        encoder.bitRate = VoiceCall.Constants.upKbps * 1000
        self.resampler = resampler
        self.encoder = encoder
        self.decoder = decoder

        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: downPcm)
        let tapFrames = AVAudioFrameCount(inFormat.sampleRate * Double(VoiceCall.Constants.frameMs) / 1000)
        input.installTap(onBus: 0, bufferSize: tapFrames, format: inFormat) { [weak self] buffer, _ in
            // Copied off the tap thread, never waited on: `stop` removes the
            // tap from the queue, which must not wait for a tap that waits
            // for the queue.
            guard let self, let copy = Self.copy(buffer) else { return }
            let box = Boxed(copy)
            self.queue.async { self.capture(box.value) }
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            stopOnQueue()
            throw VoiceCallAudioError.failed
        }
        player.play()
        running = true
        observe()
    }

    private func stopOnQueue() {
        running = false
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        engine.inputNode.removeTap(onBus: 0)
        player.stop()
        engine.stop()
        if engine.attachedNodes.contains(player) { engine.detach(player) }
        try? engine.inputNode.setVoiceProcessingEnabled(false)
        resampler = nil
        encoder = nil
        decoder = nil
        uplink.reset()
        held = []
        scheduled = 0
        prebuffering = true
        playGeneration += 1
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    /// An interruption (a phone call, Siri) drops the call; a route change
    /// that stops the engine restarts it, or drops the call when it can't.
    private func observe() {
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: AVAudioSession.interruptionNotification, object: nil, queue: nil
        ) { [weak self] note in
            let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt
            guard raw.flatMap(AVAudioSession.InterruptionType.init(rawValue:)) == .began else { return }
            guard let self else { return }
            self.queue.async { self.failOnQueue() }
        })
        observers.append(center.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            guard let self else { return }
            self.queue.async { self.restartOnQueue() }
        })
    }

    private func restartOnQueue() {
        guard running, !engine.isRunning else { return }
        do {
            engine.prepare()
            try engine.start()
            player.play()
            prebuffering = true
        } catch {
            failOnQueue()
        }
    }

    private func failOnQueue() {
        guard running else { return }
        stopOnQueue()
        onFailure("dropped")
    }

    // MARK: - Uplink

    /// The microphone closes (silence goes out, replies still play).
    func setMuted(_ muted: Bool) {
        queue.async {
            self.uplink.setMuted(muted)
            if muted { self.onLevel(0, false) }
        }
    }

    private func capture(_ buffer: AVAudioPCMBuffer) {
        guard running, let resampler, buffer.frameLength > 0 else { return }
        let ratio = upPcm.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 64
        guard let out = AVAudioPCMBuffer(pcmFormat: upPcm, frameCapacity: capacity) else { return }
        var error: NSError?
        let once = OnceInput(buffer)
        _ = resampler.convert(to: out, error: &error) { _, status in once.next(status) }
        guard error == nil, out.frameLength > 0, let samples = out.int16ChannelData?[0] else { return }
        let count = Int(out.frameLength)
        if !uplink.muted {
            var sum: Double = 0
            for i in 0..<count {
                let s = Double(samples[i]) / 32768
                sum += s * s
            }
            onLevel(Self.level(rms: (sum / Double(count)).squareRoot()), false)
        }
        // Muted = encoded silence, the relay's idle keep-alive.
        for frame in uplink.push(UnsafeBufferPointer(start: samples, count: count)) {
            encode(frame)
        }
    }

    private func encode(_ frame: [Int16]) {
        guard let encoder,
              let pcm = AVAudioPCMBuffer(pcmFormat: upPcm, frameCapacity: AVAudioFrameCount(frame.count)),
              let target = pcm.int16ChannelData?[0]
        else { return }
        pcm.frameLength = AVAudioFrameCount(frame.count)
        frame.withUnsafeBufferPointer { target.update(from: $0.baseAddress!, count: frame.count) }
        let packetSize = max(encoder.maximumOutputPacketSize, VoiceCall.Wire.maxFrameBytes)
        let out = AVAudioCompressedBuffer(format: upOpus, packetCapacity: 1, maximumPacketSize: packetSize)
        var error: NSError?
        let once = OnceInput(pcm)
        _ = encoder.convert(to: out, error: &error) { _, status in once.next(status) }
        guard error == nil, out.packetCount > 0, out.byteLength > 0 else { return }
        let packet = Data(bytes: out.data, count: Int(out.byteLength))
        let wire = VoiceCall.Wire.frame(kind: VoiceCall.Wire.kindUp, seq: seq, packet: packet)
        seq &+= 1
        // A frame above the cap is dropped by the relay anyway.
        guard wire.count <= VoiceCall.Wire.maxFrameBytes else { return }
        onUplink(wire)
    }

    // MARK: - Downlink

    /// One binary frame off the socket (any thread).
    func receive(_ frame: Data) {
        queue.async { self.play(frame) }
    }

    /// Barge-in (`call_event` speechStarted): drop the reply still buffered.
    func flush() {
        queue.async {
            guard self.running else { return }
            self.playGeneration += 1
            self.held = []
            self.scheduled = 0
            self.prebuffering = true
            self.player.stop()
            self.player.play()
            self.onLevel(0, true)
        }
    }

    private func play(_ frame: Data) {
        guard running, let decoder,
              let parsed = VoiceCall.Wire.parse(frame), parsed.kind == VoiceCall.Wire.kindDown
        else { return }
        let packet = parsed.packet
        let input = AVAudioCompressedBuffer(format: downOpus, packetCapacity: 1, maximumPacketSize: packet.count)
        packet.copyBytes(to: input.data.assumingMemoryBound(to: UInt8.self), count: packet.count)
        input.byteLength = UInt32(packet.count)
        input.packetCount = 1
        input.packetDescriptions?.pointee = AudioStreamPacketDescription(
            mStartOffset: 0, mVariableFramesInPacket: 0, mDataByteSize: UInt32(packet.count)
        )
        // Opus packets run up to 120 ms.
        let capacity = AVAudioFrameCount(VoiceCall.Constants.downRate * 120 / 1000)
        guard let pcm = AVAudioPCMBuffer(pcmFormat: downPcm, frameCapacity: capacity) else { return }
        var error: NSError?
        let once = OnceInput(input)
        _ = decoder.convert(to: pcm, error: &error) { _, status in once.next(status) }
        guard error == nil, pcm.frameLength > 0, let samples = pcm.floatChannelData?[0] else { return }
        var sum: Double = 0
        for i in 0..<Int(pcm.frameLength) {
            let s = Double(samples[i])
            sum += s * s
        }
        onLevel(Self.level(rms: (sum / Double(pcm.frameLength)).squareRoot()), true)
        if prebuffering {
            held.append(pcm)
            guard held.count >= prebuffer else { return }
            prebuffering = false
            let ready = held
            held = []
            for buffer in ready { schedule(buffer) }
        } else {
            schedule(pcm)
        }
    }

    private func schedule(_ buffer: AVAudioPCMBuffer) {
        scheduled += 1
        let generation = playGeneration
        player.scheduleBuffer(buffer) { [weak self] in
            guard let self else { return }
            self.queue.async {
                guard generation == self.playGeneration else { return }
                self.scheduled = max(0, self.scheduled - 1)
                // An underrun: hold the next frames again before playing.
                if self.scheduled == 0 { self.prebuffering = true }
            }
        }
    }

    /// A speech RMS (~0.02 quiet … ~0.25 loud) as the wave's 0..1 level.
    private static func level(rms: Double) -> Double {
        guard rms.isFinite else { return 0 }
        return min(1, rms * 5)
    }
}

extension VoiceCallAudio {
    /// A tap buffer's own copy (the engine reuses its tap buffers).
    fileprivate static func copy(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        guard let copy = AVAudioPCMBuffer(pcmFormat: buffer.format, frameCapacity: buffer.frameLength) else { return nil }
        copy.frameLength = buffer.frameLength
        let source = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: buffer.audioBufferList))
        let target = UnsafeMutableAudioBufferListPointer(copy.mutableAudioBufferList)
        for i in 0..<min(source.count, target.count) {
            guard let from = source[i].mData, let to = target[i].mData else { continue }
            let bytes = min(source[i].mDataByteSize, target[i].mDataByteSize)
            memcpy(to, from, Int(bytes))
            target[i].mDataByteSize = bytes
        }
        return copy
    }
}

/// A non-Sendable value handed across one queue hop it is confined to.
private final class Boxed<Value>: @unchecked Sendable {
    let value: Value

    init(_ value: Value) {
        self.value = value
    }
}

/// An AVAudioConverter input block's state: the one buffer, then "no data
/// now" (never end-of-stream, so the converter keeps its stream state).
private final class OnceInput: @unchecked Sendable {
    private var buffer: AVAudioBuffer?

    init(_ buffer: AVAudioBuffer) {
        self.buffer = buffer
    }

    func next(_ status: UnsafeMutablePointer<AVAudioConverterInputStatus>) -> AVAudioBuffer? {
        guard let buffer else {
            status.pointee = .noDataNow
            return nil
        }
        self.buffer = nil
        status.pointee = .haveData
        return buffer
    }
}
