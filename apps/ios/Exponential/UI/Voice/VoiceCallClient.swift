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
/// `call_end` (`hungUp`) / an error ends it (a failure with a toast). A call keeps running with the
/// screen locked or the app in the background (the `audio` background mode);
/// while one runs the screen never dims on its own.
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

    /// What the call surface shows; nil = no call.
    var view: VoiceCall.View? { VoiceCall.callView(state, platform: .phone) }

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var startTask: Task<Void, Never>?
    @ObservationIgnored private var socket: URLSessionWebSocketTask?
    @ObservationIgnored private var audio: VoiceCallAudio?
    @ObservationIgnored private let toaster = Toaster.shared
    @ObservationIgnored private let ring = CallRingTone()

    private init() {}

    // MARK: - Commands

    /// Calls `agentId` on the member's voice device (named `device`).
    func start(
        accountId: String, agentId: String, callee: VoiceCallCallee, device: String, language: String?,
        voiceApi: VoiceApi
    ) {
        guard state.phase == .hidden else { return }
        self.callee = callee
        generation += 1
        let generation = generation
        step(.start(device: device, language: language))
        CallAudioSession.setSpeaker(state.speaker)
        ring.start()
        startTask = Task { [weak self] in
            let granted = await AVAudioApplication.requestRecordPermission()
            guard let self, generation == self.generation else { return }
            guard granted else {
                self.fail("not_granted")
                return
            }
            let ticket: VoiceCall.Ticket
            do {
                ticket = try await voiceApi.startCall(accountId: accountId, agentId: agentId)
            } catch {
                guard generation == self.generation else { return }
                // The server's refusal (the availability reason, no relay).
                self.teardown()
                self.step(.end)
                self.toaster.error(error.userFacingMessage)
                return
            }
            guard generation == self.generation else { return }
            guard let url = URL(string: ticket.url) else {
                self.fail("dropped")
                return
            }
            self.dial(url, generation: generation)
        }
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

    /// Mute / Unmute: the microphone closes (silence goes up, which keeps
    /// the relay's idle timer fed), replies still play; the host hears
    /// `call_mute`.
    func toggleMute() {
        guard state.phase != .hidden else { return }
        let muted = !state.muted
        step(muted ? .mute : .unmute)
        audio?.setMuted(muted)
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

    private func dial(_ url: URL, generation: Int) {
        let task = URLSession.shared.webSocketTask(with: url)
        socket = task
        let uplink = UplinkSender(task)
        audio = VoiceCallAudio(
            onUplink: { frame in uplink.send(frame) },
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
        Self.pump(task, audio: audio, onText: { [weak self] text in
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
    /// player's queue, control frames hop to the model.
    nonisolated private static func pump(
        _ task: URLSessionWebSocketTask,
        audio: VoiceCallAudio?,
        onText: @escaping @Sendable (String) -> Void,
        onClosed: @escaping @Sendable () -> Void
    ) {
        task.receive { result in
            switch result {
            case let .success(.data(frame)):
                audio?.receive(frame)
            case let .success(.string(text)):
                onText(text)
            case .success:
                break
            case .failure:
                onClosed()
                return
            }
            pump(task, audio: audio, onText: onText, onClosed: onClosed)
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
            // Barge-in: the reply still buffered stops at once.
            if event == .speechStarted { audio?.flush() }
            step(event)
        case "call_end":
            // hangUpRules: a reason drops the call with its toast, none ends it.
            teardown()
            step(.hungUp(reason: frame["reason"] as? String))
            if let toast = state.toast { toaster.error(toast) }
        case "error":
            fail(frame["code"] as? String ?? "dropped")
        case "bye":
            // The room closed without a `call_end` first.
            fail(frame["outcome"] as? String ?? "dropped")
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
        ring.stop()
        if let audio {
            audio.stop()
        } else {
            CallAudioSession.leave()
        }
        audio = nil
        socket?.cancel(with: .normalClosure, reason: nil)
        socket = nil
        level = 0
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
