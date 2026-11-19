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
@MainActor
@Observable
final class VoiceCallClient {
    static let shared = VoiceCallClient()

    private(set) var state = VoiceCall.idle
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
    @ObservationIgnored private var startTask: Task<Void, Never>?
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
        let generation = generation
        step(.start(device: device, language: language, fallback: fallback))
        CallAudioSession.setSpeaker(state.speaker)
        ring.start()
        startTask = Task { [weak self] in
            let granted = await AVAudioApplication.requestRecordPermission()
            guard let self, generation == self.generation else { return }
            guard granted else {
                self.fail("not_granted")
                return
            }
            // A key holder that is still starting is waited for, then the
            // phone claims; one that cannot hold keys dials without the claim.
            let keys = CallSealLine.supported ? keys : nil
            let claim = await keys?.ready() ?? false
            guard generation == self.generation else { return }
            async let asked = claim ? keys?.target(agentId: agentId) : nil
            let ticket: VoiceCall.Ticket
            do {
                ticket = try await voiceApi.startCall(accountId: accountId, agentId: agentId, seal: claim)
            } catch {
                guard generation == self.generation else { return }
                // The server's refusal (the availability reason, no relay).
                self.teardown()
                self.step(.end)
                self.toaster.error(error.userFacingMessage)
                return
            }
            guard generation == self.generation else { return }
            // EXP-1857: the server picked the target itself (an older server
            // answers neither field): its answer names the device that speaks.
            if let device = ticket.device { self.state.device = device }
            if let fallback = ticket.fallback { self.state.fallback = fallback }
            let target = await asked
            guard generation == self.generation else { return }
            guard let url = URL(string: ticket.url) else {
                self.fail("dropped")
                return
            }
            // Told sealed without the claim or the ids: no clear call instead.
            var sealed: SealedCall?
            if ticket.seal == true {
                guard claim, let keys, let deviceId = ticket.deviceId else {
                    self.fail("dropped")
                    return
                }
                sealed = SealedCall(callId: ticket.callId, agentId: agentId, deviceId: deviceId, target: target, keys: keys)
            }
            self.dial(url, generation: generation, sealed: sealed)
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
        if let socket {
            socket.send(.string(#"{"t":"call_end"}"#)) { _ in
                socket.cancel(with: .normalClosure, reason: nil)
            }
            self.socket = nil
        }
        teardown()
        step(.end)
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
        let muted = !state.muted
        step(muted ? .mute : .unmute)
        audio?.setMuted(muted)
        // A sealed call sends no `call_mute`: mute rides inside every sealed frame.
        if let line {
            line.muted = muted
            return
        }
        socket?.send(.string(#"{"t":"call_mute","muted":\#(muted)}"#)) { _ in }
    }

    /// Speaker / Speaker off: the loudspeaker vs the earpiece (device-local,
    /// the host never hears of it).
    func toggleSpeaker() {
        guard state.phase != .hidden else { return }
        step(.speaker(on: !state.speaker))
        CallAudioSession.setSpeaker(state.speaker)
    }

    // MARK: - Socket

    private func dial(_ url: URL, generation: Int, sealed: SealedCall?) {
        let task = URLSession.shared.webSocketTask(with: url)
        socket = task
        if let sealed {
            let io = CallSealLine.Io(
                sendBinary: { frame in task.send(.data(frame)) { _ in } },
                end: { reason in task.send(.string(#"{"t":"call_end","reason":"\#(reason)"}"#)) { _ in } },
                fail: { [weak self] code in
                    Task { @MainActor in
                        guard let self, generation == self.generation else { return }
                        self.fail(code)
                    }
                }
            )
            line = CallSealLine.make(
                callId: sealed.callId, agentId: sealed.agentId, deviceId: sealed.deviceId,
                target: sealed.target, keys: sealed.keys, io: io
            )
            guard line != nil else {
                fail("dropped")
                return
            }
        }
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
                    self.fail("dropped")
                    return
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
                    self.fail(code)
                }
            }
        )
        task.resume()
        Self.pump(task, audio: audio, line: line, liveness: liveness, onText: { [weak self] text in
            Task { @MainActor in
                guard let self, generation == self.generation else { return }
                self.handle(text)
            }
        }, onClosed: { [weak self] in
            Task { @MainActor in
                guard let self, generation == self.generation else { return }
                // The socket went away without `call_end` or `bye`.
                logger.info("call socket closed without an end")
                self.fail("dropped")
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

    /// One control frame (`wire.control`, plus the relay's `error` / `bye`).
    private func handle(_ text: String) {
        guard let data = text.data(using: .utf8),
              let frame = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else { return }
        switch frame["t"] as? String {
        case "call_ready":
            // The pair is up: the flow keeps ringing until the host's rang +
            // ready; our stream is what the host warms up on.
            guard let audio else { return }
            let generation = generation
            Task { [weak self] in
                do {
                    try await audio.start()
                    // Ended while the engine started: give the session back.
                    if generation != self?.generation { audio.stop() }
                } catch {
                    guard let self, generation == self.generation else { return }
                    self.fail(error as? VoiceCallAudioError == .noMicrophone ? "no_microphone" : "dropped")
                }
            }
        case "call_event":
            guard let json = frame["event"] as? [String: Any],
                  let event = VoiceCall.Event(json: json), event.isHostEvent
            else { return }
            // The host could not get its voice ready: it fails the call
            // with its code before its `call_end` (an empty code reads as
            // `dropped`, voice-call.json).
            if case let .failed(code) = event {
                fail(code.isEmpty ? "dropped" : VoiceCall.Seal.wireCode(code))
                return
            }
            // Barge-in: the reply still buffered stops at once.
            if event == .speechStarted { audio?.flush() }
            step(event)
        case "call_end":
            // hangUpRules: a reason drops the call with its toast, none ends it.
            teardown()
            step(.hungUp(reason: frame["reason"] as? String))
            if let toast = state.toast { toaster.error(toast) }
        case "error":
            fail(VoiceCall.Seal.wireCode(frame["code"] as? String ?? "dropped"))
        case "call_alive":
            liveness?.beat()
        case "bye":
            // The room closed without a `call_end` first.
            fail(VoiceCall.Seal.wireCode(frame["outcome"] as? String ?? "dropped"))
        default:
            break
        }
    }

    // MARK: - State

    private func step(_ event: VoiceCall.Event) {
        let before = state.phase
        state = VoiceCall.callStep(state, event)
        if state.phase != before { level = 0 }
        if before == .ringing, state.phase != .ringing {
            ring.stop()
            answeredAt = state.phase == .hidden ? nil : Date()
        }
        if state.phase == .hidden { answeredAt = nil }
        UIApplication.shared.isIdleTimerDisabled = state.phase != .hidden
    }

    /// Ends the call with its failure toast.
    private func fail(_ code: String) {
        guard state.phase != .hidden else { return }
        teardown()
        step(.failed(code: code))
        if let toast = state.toast { toaster.error(toast) }
    }

    /// Every end: the socket, the engine and the session go; late callbacks
    /// of this call turn inert.
    private func teardown() {
        generation += 1
        startTask?.cancel()
        startTask = nil
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
