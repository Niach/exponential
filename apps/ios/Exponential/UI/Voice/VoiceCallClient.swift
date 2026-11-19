import AVFoundation
import ExpCore
import ExpUI
import Foundation
import os
import UIKit

private let logger = Logger(subsystem: "at.exponential", category: "voice-call")

/// NEW-69 (voice V5): THE phone's call with an agent, streamed to the
/// member's voice device (fixtures/voice-call.json TRANSPORT). One per app:
/// the in-call screen is mounted at the root (`AppNavigator`), so a call
/// outlives the chat that started it (the first utterance can open the
/// conversation, which swaps the agent's page underneath).
///
/// The flow (S-3): `start(device, language)` RINGS (a local ring tone) →
/// microphone permission → `voice.startCall({agentId, platform: phone})` →
/// dial the answered `url` as-is → `call_ready` starts the capture (the
/// host warms up on our stream) → the host's `call_event`s (rang, ready,
/// noSound, then the turns) fold into `VoiceCall.callStep`; rang + ready
/// answer the call (the ring stops, the timer starts) → End call /
/// `call_end` (`hungUp`) / an error ends it (a failure with a toast). The app leaving the foreground
/// (screen locked, another app in front) ends the call: no `audio`
/// background mode (App Review 2.5.4); while one runs the screen never dims
/// on its own.
///
/// #EXP-1941: A SEALED CALL. A phone that can hold keys claims `seal`; when
/// `voice.startCall` answers `seal: true` the call speaks the sealed wire
/// through `CallSealLine`: frames are opened in `pump`, in arrival order,
/// before the hop to the main actor; the capture thread's every packet
/// passes `uplink`, which seals only after the host proved the chat's key
/// (before that nothing of the microphone leaves); mute rides inside every
/// frame (no `call_mute`). A seal failure reads `dropped`, and a wire code
/// about this phone's own device is never taken from the wire.
///
/// #EXP-1993: WHAT happens is `VoiceCall.Caller` (voice-call.json `caller`,
/// replayed with Android's): every command, socket frame and failure goes
/// in through `send`, and this class only does the effects, in order.
@MainActor
@Observable
final class VoiceCallClient {
    static let shared = VoiceCallClient()

    /// The caller's session (`VoiceCall.Caller`): the flow and its stage.
    private(set) var caller = VoiceCall.Caller.State.idle
    var state: VoiceCall.State { caller.call }
    /// The wave's level: the microphone while listening, the reply while
    /// speaking.
    private(set) var level: Double = 0

    /// Who is called (the screen's avatar and name).
    private(set) var callee = VoiceCallCallee(name: "")
    /// When the call was answered (the timer's zero); nil while ringing.
    private(set) var answeredAt: Date?
    /// EXP-1681: the call screen is minimized into the floating pill.
    private(set) var minimized = false
    /// FEED-272: the tab on top of the call (a call opens on Voice).
    private(set) var tab: VoiceCall.Tab = VoiceCall.Tabs.initial
    /// The called agent's definition id (its chat is the Chat tab's).
    private(set) var agentId: String?
    /// FEED-272: the agent chats on screen now (each registers while shown).
    private(set) var chatsShown: [String: Int] = [:]

    /// What the call surface shows; nil = no call.
    var view: VoiceCall.View? { VoiceCall.callView(state, platform: .phone) }

    @ObservationIgnored private var generation = 0
    /// Inputs an effect handed in wait for the effects before them.
    @ObservationIgnored private var inbox: [VoiceCall.Caller.Input] = []
    @ObservationIgnored private var stepping = false
    /// What the `ticket` and `dial` effects need, set by `start`.
    @ObservationIgnored private var asked: Asked?
    @ObservationIgnored private var dialling: Dialling?
    @ObservationIgnored private var startTask: Task<Void, Never>?
    /// The ring's bound (`ring`'s `ms`).
    @ObservationIgnored private var ringTask: Task<Void, Never>?
    @ObservationIgnored private var socket: URLSessionWebSocketTask?
    @ObservationIgnored private var audio: VoiceCallAudio?
    /// #EXP-1941: the sealed line of a call the server marked sealed; nil = today's wire.
    @ObservationIgnored private var line: CallSealLine?
    /// EXP-1929: what the relay last sent, and whether it beats.
    @ObservationIgnored private var liveness: CallLiveness?
    @ObservationIgnored private var livenessTask: Task<Void, Never>?
    @ObservationIgnored private let toaster = Toaster.shared
    @ObservationIgnored private let ring = CallRingTone()

    private init() {}

    // MARK: - Commands

    /// Calls `agentId` on the desktop named `device` (EXP-1857: `callTarget`,
    /// the agent's own desktop or, `fallback`, another of the member's).
    func start(
        accountId: String, agentId: String, callee: VoiceCallCallee, device: String, language: String?,
        fallback: Bool, voiceApi: VoiceApi, keys: CallKeySource?
    ) {
        guard state.phase == .hidden else { return }
        self.callee = callee
        // A new call always opens full screen, on Voice.
        minimized = false
        tab = VoiceCall.Tabs.initial
        self.agentId = agentId
        generation += 1
        asked = Asked(accountId: accountId, agentId: agentId, voiceApi: voiceApi, keys: keys)
        send(.start(device: device, language: language, fallback: fallback))
    }

    private struct Asked: Sendable {
        let accountId: String
        let agentId: String
        let voiceApi: VoiceApi
        let keys: CallKeySource?
    }

    private struct Dialling {
        let url: URL
        let sealed: SealedCall?
    }

    /// The `ticket` effect: the microphone's permission (asked once the ring
    /// started), a key holder, then `voice.startCall`.
    private func askTicket() {
        guard let asked else { return }
        let generation = generation
        startTask = Task { [weak self] in
            let granted = await AVAudioApplication.requestRecordPermission()
            guard let self, generation == self.generation else { return }
            guard granted else { return self.send(.failed(code: "not_granted", told: false)) }
            // A key holder that is still starting is waited for, then the
            // phone claims; one that cannot hold keys dials without the claim.
            let keys = CallSealLine.supported ? asked.keys : nil
            let claim = await keys?.ready() ?? false
            guard generation == self.generation else { return }
            async let target = claim ? keys?.target(agentId: asked.agentId) : nil
            let ticket: VoiceCall.Ticket
            do {
                ticket = try await asked.voiceApi.startCall(accountId: asked.accountId, agentId: asked.agentId, seal: claim)
            } catch {
                guard generation == self.generation else { return }
                // The server's refusal (the availability reason, no relay).
                return self.send(.refused(message: error.userFacingMessage))
            }
            let sealTarget = await target
            guard generation == self.generation else { return }
            guard let url = URL(string: ticket.url) else { return self.send(.failed(code: "dropped", told: false)) }
            // Told sealed without the claim or the ids: no clear call instead.
            var sealed: SealedCall?
            if ticket.seal == true {
                guard claim, let keys, let deviceId = ticket.deviceId else { return self.send(.failed(code: "dropped", told: false)) }
                sealed = SealedCall(callId: ticket.callId, agentId: asked.agentId, deviceId: deviceId, target: sealTarget, keys: keys)
            }
            self.dialling = Dialling(url: url, sealed: sealed)
            // EXP-1857: the server picked the target itself (an older server
            // answers neither field): its answer names the device that speaks.
            self.send(.ticket(device: ticket.device, fallback: ticket.fallback, sealed: sealed != nil))
        }
    }

    private struct SealedCall {
        let callId: String
        let agentId: String
        let deviceId: String
        let target: ServerSendTarget?
        let keys: CallKeySource
    }

    /// End call: `call_end` to the host, then close.
    func end() {
        guard state.phase != .hidden else { return }
        send(.end)
    }

    /// EXP-1681: Minimize (the chevron, a swipe down): the screen goes, the
    /// floating pill shows, the call goes on.
    func minimize() {
        guard state.phase != .hidden else { return }
        minimized = true
    }

    /// A tap on the pill: the call screen again (on Voice).
    func expand() {
        minimized = false
        tab = .voice
    }

    /// FEED-272: Chat / Voice; never ends, mutes or re-dials the call.
    func showTab(_ tab: VoiceCall.Tab) {
        guard state.phase != .hidden else { return }
        self.tab = tab
        minimized = false
    }

    /// FEED-272: the call runs for `agentId` on its Chat tab: that agent's
    /// chat draws the tabs and the pill's row docked in place of its header.
    func docked(agentId: String?) -> Bool {
        agentId != nil && view != nil && !minimized && tab == .chat && self.agentId == agentId
    }

    /// The Chat tab while no chat of the call's agent shows (the root opens it).
    var chatMissing: Bool {
        guard view != nil, !minimized, tab == .chat, let agentId else { return false }
        return chatsShown[agentId] == nil
    }

    /// An agent chat appears / leaves the screen. Leaving the call's chat on
    /// its Chat tab minimizes the call into the pill.
    func chatShown(agentId: String, _ shown: Bool) {
        let count = (chatsShown[agentId] ?? 0) + (shown ? 1 : -1)
        chatsShown[agentId] = count > 0 ? count : nil
        if !shown, chatsShown[agentId] == nil, self.agentId == agentId, tab == .chat { minimize() }
    }

    /// Mute / Unmute: the microphone closes (silence goes up, which keeps
    /// the relay's idle timer fed), replies still play; the host hears
    /// `call_mute`.
    func toggleMute() {
        guard state.phase != .hidden else { return }
        send(.mute)
    }

    /// Speaker / Speaker off: the loudspeaker vs the earpiece (device-local,
    /// the host never hears of it).
    func toggleSpeaker() {
        guard state.phase != .hidden else { return }
        send(.speaker)
    }

    // MARK: - Socket

    private func dial() {
        guard let dialling else { return }
        let generation = generation
        let task = URLSession.shared.webSocketTask(with: dialling.url)
        if let sealed = dialling.sealed {
            let io = CallSealLine.Io(
                sendBinary: { frame in task.send(.data(frame)) { _ in } },
                end: { reason in task.send(.string(#"{"t":"call_end","reason":"\#(reason)"}"#)) { _ in } },
                // The line sent its own `call_end {reason}`.
                fail: { [weak self] code in
                    Task { @MainActor in
                        guard let self, generation == self.generation else { return }
                        self.send(.failed(code: code, told: true))
                    }
                }
            )
            line = CallSealLine.make(
                callId: sealed.callId, agentId: sealed.agentId, deviceId: sealed.deviceId,
                target: sealed.target, keys: sealed.keys, io: io
            )
            // Never resumed: nothing connected.
            guard line != nil else { return send(.failed(code: "dropped", told: false)) }
        }
        socket = task
        let line = line
        let liveness = CallLiveness()
        self.liveness = liveness
        // EXP-1929: once the relay beat, a path that carries nothing for
        // `idleTimeoutMs` is dead: the call ends dropped.
        livenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, generation == self.generation else { return }
                if liveness.dead() {
                    logger.info("call path went silent")
                    return self.send(.failed(code: "dropped", told: false))
                }
            }
        }
        let uplink = UplinkSender(task)
        audio = VoiceCallAudio(
            // A sealed call's ONE gate: a frame leaves only through the sealer.
            onUplink: { frame in
                guard let line else { return uplink.send(frame) }
                if let sealed = line.uplink(frame) { uplink.send(sealed) }
            },
            onLevel: { [weak self] level, speaking in
                Task { @MainActor in
                    guard let self, generation == self.generation else { return }
                    // Each source drives the wave only in its own phase.
                    guard speaking == (self.state.phase == .speaking) else { return }
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
        task.resume()
        Self.pump(task, audio: audio, line: line, liveness: liveness, onText: { [weak self] text in
            Task { @MainActor in
                guard let self, generation == self.generation, let frame = VoiceCall.Caller.Frame(text: text) else { return }
                self.send(.frame(frame))
            }
        }, onClosed: { [weak self] in
            Task { @MainActor in
                guard let self, generation == self.generation else { return }
                // The socket went away without `call_end` or `bye`.
                logger.info("call socket closed without an end")
                self.send(.closed)
            }
        })
    }

    /// The receive loop, off the main actor: audio goes straight to the
    /// player's queue, control frames hop to the model. A sealed call's
    /// frames are opened HERE, in arrival order, before either hop.
    nonisolated private static func pump(
        _ task: URLSessionWebSocketTask,
        audio: VoiceCallAudio?,
        line: CallSealLine?,
        liveness: CallLiveness,
        onText: @escaping @Sendable (String) -> Void,
        onClosed: @escaping @Sendable () -> Void
    ) {
        task.receive { result in
            if case .success = result { liveness.heard() }
            switch result {
            case let .success(.data(frame)):
                if let opened = line == nil ? frame : line?.inbound(frame: frame) { audio?.receive(opened) }
            case let .success(.string(text)):
                if let opened = line == nil ? text : line?.inbound(text: text) { onText(opened) }
            case .success:
                break
            case .failure:
                onClosed()
                return
            }
            pump(task, audio: audio, line: line, liveness: liveness, onText: onText, onClosed: onClosed)
        }
    }

    // MARK: - The caller's machine

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
            UIApplication.shared.isIdleTimerDisabled = state.phase != .hidden
            stepped.effects.forEach(perform)
        }
    }

    private func perform(_ effect: VoiceCall.Caller.Effect) {
        switch effect {
        case .hold:
            // iOS holds nothing (no `audio` background mode): the call rings at once.
            CallAudioSession.setSpeaker(state.speaker)
            send(.held(ok: true, code: nil))
        case let .ring(ms):
            ring.start()
            let generation = generation
            ringTask = Task { [weak self] in
                try? await Task.sleep(for: .milliseconds(ms))
                guard let self, !Task.isCancelled, generation == self.generation else { return }
                self.send(.ringTimeout)
            }
        case .ticket:
            askTicket()
        case .dial:
            dial()
        case .startAudio:
            // The pair is up: our stream is what the host warms up on.
            guard let audio else { return }
            let generation = generation
            Task { [weak self] in
                do {
                    try await audio.start()
                    // Ended while the engine started: give the session back.
                    if generation != self?.generation { audio.stop() }
                } catch {
                    guard let self, generation == self.generation else { return }
                    self.send(.failed(code: error as? VoiceCallAudioError == .noMicrophone ? "no_microphone" : "dropped", told: false))
                }
            }
        case .stopRing:
            ring.stop()
            ringTask?.cancel()
            ringTask = nil
        case .flush:
            // Barge-in: the reply still buffered stops at once.
            audio?.flush()
        case let .mute(muted):
            audio?.setMuted(muted)
            line?.muted = muted
        case let .speaker(on):
            CallAudioSession.setSpeaker(on)
        case let .send(frame):
            guard let socket else { return }
            if case .end = frame {
                // The last frame: the socket closes once it went out.
                self.socket = nil
                socket.send(.string(frame.text)) { _ in socket.cancel(with: .normalClosure, reason: nil) }
            } else {
                socket.send(.string(frame.text)) { _ in }
            }
        case .alive:
            liveness?.beat()
        case .release:
            teardown()
        case let .toast(text):
            toaster.error(text)
        }
    }

    /// Every end: the socket, the engine and the session go; late callbacks
    /// of this call turn inert.
    private func teardown() {
        generation += 1
        asked = nil
        dialling = nil
        startTask?.cancel()
        startTask = nil
        ringTask?.cancel()
        ringTask = nil
        livenessTask?.cancel()
        livenessTask = nil
        liveness = nil
        ring.stop()
        if let audio {
            audio.stop()
        } else {
            // On the audio queue like every enter (the ring's included).
            VoiceCallAudio.sharedQueue.async { CallAudioSession.leave() }
        }
        audio = nil
        line?.close()
        line = nil
        socket?.cancel(with: .normalClosure, reason: nil)
        socket = nil
        level = 0
    }
}

/// EXP-1929: when the relay last sent anything (the receive loop, off the
/// main actor) and whether it beats (`call_alive`).
private final class CallLiveness: @unchecked Sendable {
    private let lock = NSLock()
    private var alive = false
    private var heardAt = Date()

    func heard() {
        lock.withLock { heardAt = Date() }
    }

    func beat() {
        lock.withLock { alive = true }
    }

    func dead() -> Bool {
        lock.withLock { VoiceCall.callPathDead(alive: alive, silentMs: Int(Date().timeIntervalSince(heardAt) * 1000)) }
    }
}

/// The uplink's socket sends through `VoiceCall.Outbox`: one frame in
/// flight, a bounded backlog that drops its oldest frames on a stall.
private final class UplinkSender: @unchecked Sendable {
    private weak var task: URLSessionWebSocketTask?
    private let lock = NSLock()
    private var outbox = VoiceCall.Outbox<Data>()

    init(_ task: URLSessionWebSocketTask) {
        self.task = task
    }

    func send(_ frame: Data) {
        guard let now = lock.withLock({ outbox.offer(frame) }) else { return }
        write(now)
    }

    private func write(_ frame: Data) {
        guard let task else { return }
        task.send(.data(frame)) { [weak self] error in
            // A failed send = the socket is gone; its receive loop ends the call.
            guard let self, error == nil else { return }
            if let next = self.lock.withLock({ self.outbox.sent() }) { self.write(next) }
        }
    }
}
