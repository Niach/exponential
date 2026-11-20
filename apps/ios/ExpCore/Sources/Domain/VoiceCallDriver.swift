import Foundation
import Observation
import os

private let logger = Logger(subsystem: "at.exponential", category: "voice-call")

// #EXP-1993: the phone's call as the app runs it. `VoiceCall.Caller` decides
// (voice-call.json `caller`); `VoiceCallDriver` does each effect through
// small ports (the socket, the audio engine, the seal line, the platform),
// in order, turns what they report into the machine's inputs and drops
// whatever a call reports after its end (the generation). The app's
// `VoiceCallClient` is only the iOS glue behind the ports;
// `VoiceCallDriverTests` replays every `caller` case through this driver
// with fakes and asserts the calls it makes.

/// One dialled socket (`URLSessionWebSocketTask` in the app).
public protocol CallSocket: AnyObject, Sendable {
    /// A control frame.
    func send(text: String)
    /// The call's last control frame: the socket closes once it went out.
    func sendLast(text: String)
    /// A binary frame as it is (the seal line's hello).
    func send(binary: Data)
    /// One captured up frame (the app's bounded backlog drops the oldest on a stall).
    func send(audio: Data)
    /// Receiving starts: every frame goes to `inbound`, in arrival order.
    func resume(_ inbound: CallInbound)
    func close()
}

/// The call's microphone and playback (`VoiceCallAudio` in the app).
public protocol CallAudio: AnyObject, Sendable {
    /// Throws `CallAudioError.noMicrophone` when there is none.
    func open() async throws
    func stop()
    func setMuted(_ muted: Bool)
    /// Barge-in: the buffered reply goes.
    func flush()
    /// One down frame (`kind`, `seq`, packet) for the player.
    func receive(_ frame: Data)
}

public enum CallAudioError: Error, Equatable {
    case noMicrophone
}

/// The sealed line of a call the server marked sealed.
public protocol CallSeal: AnyObject, Sendable {
    func inbound(text: String) -> String?
    func inbound(frame: Data) -> Data?
    /// THE capture path's gate: nil = the frame does not leave.
    func uplink(_ clear: Data) -> Data?
    var muted: Bool { get set }
    func close()
}

extension CallSealLine: CallSeal {}

/// EXP-1929: when the relay last sent anything and whether it beats.
public protocol CallPathWatch: AnyObject, Sendable {
    func heard()
    func beat()
    /// `callPathDead`.
    func dead() -> Bool
}

public protocol CallTimer {
    func cancel()
}

/// What a call needs from the platform.
@MainActor
public protocol CallPlatform: AnyObject {
    /// What the call runs on before it rings, answered once (iOS: nothing,
    /// the speaker's route set, answered at once).
    func hold(speaker: Bool, answer: @escaping @MainActor (_ ok: Bool, _ code: String?) -> Void)
    func ringStart()
    func ringStop()
    func setSpeaker(_ on: Bool)
    func toast(_ text: String)
    /// The end of a call that never made an audio engine: the session goes back.
    func leaveAudioSession()
    func requestMicrophone() async -> Bool
    func startCall(agentId: String, seal: Bool) async throws -> VoiceCall.Ticket
    /// A refused `startCall` in this platform's words.
    func refusal(_ error: Error) -> String
    /// Made, not resumed.
    func openSocket(_ url: URL) -> CallSocket
    /// nil = this build cannot seal.
    func makeSeal(_ sealed: VoiceCallDriver.Sealed, io: CallSealLine.Io) -> CallSeal?
    func makeAudio(
        onUplink: @escaping @Sendable (Data) -> Void,
        onLevel: @escaping @Sendable (_ level: Double, _ speaking: Bool) -> Void,
        onFailure: @escaping @Sendable (_ code: String) -> Void
    ) -> CallAudio
    func watchPath() -> CallPathWatch
    func after(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer
    func every(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer
    /// While a call shows, the screen never dims on its own.
    func keepAwake(_ awake: Bool)
}

/// A socket's frames, off the main actor: a sealed call's are opened HERE,
/// in arrival order, before the audio goes to the player and a control
/// frame hops to the driver.
public final class CallInbound: Sendable {
    private let seal: CallSeal?
    private let audio: CallAudio?
    private let watch: CallPathWatch
    private let onText: @Sendable (String) -> Void
    private let onClosed: @Sendable () -> Void

    init(seal: CallSeal?, audio: CallAudio?, watch: CallPathWatch, onText: @escaping @Sendable (String) -> Void, onClosed: @escaping @Sendable () -> Void) {
        self.seal = seal
        self.audio = audio
        self.watch = watch
        self.onText = onText
        self.onClosed = onClosed
    }

    public func text(_ text: String) {
        watch.heard()
        if let opened = seal == nil ? text : seal?.inbound(text: text) { onText(opened) }
    }

    public func data(_ frame: Data) {
        watch.heard()
        if let opened = seal == nil ? frame : seal?.inbound(frame: frame) { audio?.receive(opened) }
    }

    /// Any other frame still says the path lives.
    public func other() {
        watch.heard()
    }

    /// The socket went away.
    public func closed() {
        onClosed()
    }
}

/// EXP-1929: the app's `CallPathWatch`.
public final class CallLiveness: CallPathWatch, @unchecked Sendable {
    private let lock = NSLock()
    private var alive = false
    private var heardAt = Date()

    public init() {}

    public func heard() {
        lock.withLock { heardAt = Date() }
    }

    public func beat() {
        lock.withLock { alive = true }
    }

    public func dead() -> Bool {
        lock.withLock { VoiceCall.callPathDead(alive: alive, silentMs: Int(Date().timeIntervalSince(heardAt) * 1000)) }
    }
}

@MainActor
@Observable
public final class VoiceCallDriver {
    /// A call the server marked sealed: what its line is made of.
    public struct Sealed: Sendable {
        public let callId: String
        public let agentId: String
        public let deviceId: String
        public let target: ServerSendTarget?
        public let keys: CallKeySource
    }

    /// The caller's session: the flow and its stage.
    public private(set) var caller = VoiceCall.Caller.State.idle
    /// The wave's level: the microphone while listening, the reply while speaking.
    public private(set) var level: Double = 0
    /// When the call was answered (the timer's zero); nil while ringing.
    public private(set) var answeredAt: Date?

    public var state: VoiceCall.State { caller.call }

    private let platform: CallPlatform
    @ObservationIgnored private var generation = 0
    /// Inputs an effect handed in wait for the effects before them.
    @ObservationIgnored private var inbox: [VoiceCall.Caller.Input] = []
    @ObservationIgnored private var stepping = false
    @ObservationIgnored private var asked: Asked?
    @ObservationIgnored private var dialling: Dialling?
    @ObservationIgnored private var askTask: Task<Void, Never>?
    @ObservationIgnored private var ringTimer: CallTimer?
    @ObservationIgnored private var watchTimer: CallTimer?
    @ObservationIgnored private var watch: CallPathWatch?
    @ObservationIgnored private var socket: CallSocket?
    @ObservationIgnored private var audio: CallAudio?
    @ObservationIgnored private var seal: CallSeal?

    private struct Asked: Sendable {
        let agentId: String
        let keys: CallKeySource?
    }

    private struct Dialling {
        let url: URL
        let sealed: Sealed?
    }

    public init(platform: CallPlatform) {
        self.platform = platform
    }

    // MARK: - Commands

    /// The tap: call `agentId` on the desktop named `device` (EXP-1857:
    /// `callTarget`); `keys` = this phone's key holder (nil = it cannot seal).
    public func start(agentId: String, keys: CallKeySource?, device: String, language: String?, fallback: Bool) {
        guard state.phase == .hidden else { return }
        generation += 1
        asked = Asked(agentId: agentId, keys: keys)
        send(.start(device: device, language: language, fallback: fallback))
    }

    /// End call (also an account switch, the app leaving the foreground).
    public func end() {
        guard state.phase != .hidden else { return }
        send(.end)
    }

    public func toggleMute() {
        guard state.phase != .hidden else { return }
        send(.mute)
    }

    public func toggleSpeaker() {
        guard state.phase != .hidden else { return }
        send(.speaker)
    }

    // MARK: - The machine

    /// Hands `input` to `VoiceCall.Caller` and does its effects, in order.
    private func send(_ input: VoiceCall.Caller.Input) {
        inbox.append(input)
        guard !stepping else { return }
        stepping = true
        defer { stepping = false }
        while !inbox.isEmpty {
            let before = state.phase
            let stepped = VoiceCall.Caller.step(caller, inbox.removeFirst())
            caller = stepped.state
            if state.phase != before { level = 0 }
            if state.phase == .hidden {
                answeredAt = nil
            } else if before == .ringing, state.phase != .ringing {
                answeredAt = Date()
            }
            platform.keepAwake(state.phase != .hidden)
            stepped.effects.forEach(perform)
        }
    }

    private func perform(_ effect: VoiceCall.Caller.Effect) {
        let generation = generation
        switch effect {
        case .hold:
            platform.hold(speaker: state.speaker) { [weak self] ok, code in
                guard let self, generation == self.generation else { return }
                self.send(.held(ok: ok, code: code))
            }
        case let .ring(ms):
            platform.ringStart()
            ringTimer = platform.after(ms: ms) { [weak self] in
                guard let self, generation == self.generation else { return }
                self.send(.ringTimeout)
            }
        case .ticket:
            ask()
        case .dial:
            dial()
        case .startAudio:
            guard let audio else { return }
            Task { [weak self] in
                do {
                    try await audio.open()
                    // Ended while the engine started: give the session back.
                    if generation != self?.generation { audio.stop() }
                } catch {
                    guard let self, generation == self.generation else { return }
                    self.send(.failed(code: error as? CallAudioError == .noMicrophone ? "no_microphone" : "dropped", told: false))
                }
            }
        case .stopRing:
            ringTimer?.cancel()
            ringTimer = nil
            platform.ringStop()
        case .flush:
            audio?.flush()
        case let .mute(muted):
            audio?.setMuted(muted)
            seal?.muted = muted
        case let .speaker(on):
            platform.setSpeaker(on)
        case let .send(frame):
            guard let socket else { return }
            if case .end = frame {
                self.socket = nil
                socket.sendLast(text: frame.text)
            } else {
                socket.send(text: frame.text)
            }
        case .alive:
            watch?.beat()
        case .release:
            teardown()
        case let .toast(text):
            platform.toast(text)
        }
    }

    /// The `ticket` effect: the microphone's permission (asked once the ring
    /// started), a key holder that is still starting, then `voice.startCall`.
    private func ask() {
        guard let asked else { return }
        let generation = generation
        askTask = Task { [weak self] in
            guard let granted = await self?.platform.requestMicrophone(), let self, generation == self.generation else { return }
            guard granted else { return self.send(.failed(code: "not_granted", told: false)) }
            let keys = asked.keys
            let claim = await keys?.ready() ?? false
            guard generation == self.generation else { return }
            async let target = claim ? keys?.target(agentId: asked.agentId) : nil
            let ticket: VoiceCall.Ticket
            do {
                ticket = try await self.platform.startCall(agentId: asked.agentId, seal: claim)
            } catch {
                guard generation == self.generation else { return }
                // The server's refusal (the availability reason, no relay).
                return self.send(.refused(message: self.platform.refusal(error)))
            }
            let sealTarget = await target
            guard generation == self.generation else { return }
            guard let url = URL(string: ticket.url) else { return self.send(.failed(code: "dropped", told: false)) }
            // Told sealed without the claim or the ids: no clear call instead.
            var sealed: Sealed?
            if ticket.seal == true {
                guard claim, let keys, let deviceId = ticket.deviceId else { return self.send(.failed(code: "dropped", told: false)) }
                sealed = Sealed(callId: ticket.callId, agentId: asked.agentId, deviceId: deviceId, target: sealTarget, keys: keys)
            }
            self.dialling = Dialling(url: url, sealed: sealed)
            // EXP-1857: the server's answer names the device that speaks.
            self.send(.ticket(device: ticket.device, fallback: ticket.fallback, sealed: sealed != nil))
        }
    }

    private func dial() {
        guard let dialling else { return }
        let generation = generation
        let socket = platform.openSocket(dialling.url)
        var made: CallSeal?
        if let sealed = dialling.sealed {
            let io = CallSealLine.Io(
                sendBinary: { frame in socket.send(binary: frame) },
                end: { reason in socket.send(text: VoiceCall.Caller.Outgoing.end(reason: reason).text) },
                // The line sent its own `call_end {reason}`.
                fail: { [weak self] code in
                    Task { @MainActor in
                        guard let self, generation == self.generation else { return }
                        self.send(.failed(code: code, told: true))
                    }
                }
            )
            made = platform.makeSeal(sealed, io: io)
            // Never resumed: nothing connected.
            guard made != nil else { return send(.failed(code: "dropped", told: false)) }
        }
        let seal = made
        self.socket = socket
        self.seal = seal
        let watch = platform.watchPath()
        self.watch = watch
        // EXP-1929: once the relay beat, a path that carries nothing for
        // `idleTimeoutMs` is dead: the call ends dropped.
        watchTimer = platform.every(ms: 1000) { [weak self] in
            guard let self, generation == self.generation, watch.dead() else { return }
            logger.info("call path went silent")
            self.send(.failed(code: "dropped", told: false))
        }
        let audio = platform.makeAudio(
            // A sealed call's ONE gate: a frame leaves only through the sealer.
            onUplink: { frame in
                guard let seal else { return socket.send(audio: frame) }
                if let sealed = seal.uplink(frame) { socket.send(audio: sealed) }
            },
            onLevel: { [weak self] level, speaking in
                Task { @MainActor in
                    // Each source drives the wave only in its own phase.
                    guard let self, generation == self.generation, speaking == (self.state.phase == .speaking) else { return }
                    self.level = level
                }
            },
            onFailure: { [weak self] code in
                Task { @MainActor in
                    guard let self, generation == self.generation else { return }
                    self.send(.failed(code: code, told: false))
                }
            }
        )
        self.audio = audio
        socket.resume(CallInbound(
            seal: seal, audio: audio, watch: watch,
            onText: { [weak self] text in
                Task { @MainActor in
                    guard let self, generation == self.generation, let frame = VoiceCall.Caller.Frame(text: text) else { return }
                    self.send(.frame(frame))
                }
            },
            onClosed: { [weak self] in
                Task { @MainActor in
                    guard let self, generation == self.generation else { return }
                    logger.info("call socket closed without an end")
                    self.send(.closed)
                }
            }
        ))
    }

    /// The `release` effect: the socket, the engine and the session go;
    /// late reports of this call turn inert.
    private func teardown() {
        generation += 1
        asked = nil
        dialling = nil
        askTask?.cancel()
        askTask = nil
        ringTimer?.cancel()
        ringTimer = nil
        watchTimer?.cancel()
        watchTimer = nil
        watch = nil
        platform.ringStop()
        if let audio {
            audio.stop()
        } else {
            platform.leaveAudioSession()
        }
        audio = nil
        seal?.close()
        seal = nil
        socket?.close()
        socket = nil
        level = 0
    }
}
