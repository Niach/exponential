import AVFoundation
import ExpCore
import ExpUI
import Foundation
import UIKit

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
/// `call_end` (`hungUp`) / an error ends it (a failure with a toast).
/// #EXP-1993: all of that is ExpCore's `VoiceCallDriver` (on
/// `VoiceCall.Caller`, voice-call.json `caller`, replayed with Android's);
/// this file is its iOS glue: URLSession, AVFoundation, the toaster. The app leaving the foreground
/// (screen locked, another app in front) ends the call: no `audio`
/// background mode (App Review 2.5.4); while one runs the screen never dims
/// on its own.
///
/// #EXP-1941: A SEALED CALL. A phone that can hold keys claims `seal`; when
/// `voice.startCall` answers `seal: true` the call speaks the sealed wire
/// through `CallSealLine`: frames are opened off the main actor, in arrival
/// order (`CallInbound`); the capture thread's every packet passes the
/// line's `uplink`, which seals only after the host proved the chat's key
/// (before that nothing of the microphone leaves); mute rides inside every
/// frame (no `call_mute`). A seal failure reads `dropped`, and a wire code
/// about this phone's own device is never taken from the wire.
@MainActor
@Observable
final class VoiceCallClient {
    static let shared = VoiceCallClient()

    private let platform: PhoneCallPlatform
    private let driver: VoiceCallDriver

    var state: VoiceCall.State { driver.state }
    /// The wave's level: the microphone while listening, the reply while
    /// speaking.
    var level: Double { driver.level }
    /// When the call was answered (the timer's zero); nil while ringing.
    var answeredAt: Date? { driver.answeredAt }

    /// Who is called (the screen's avatar and name).
    private(set) var callee = VoiceCallCallee(name: "")
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

    private init() {
        let platform = PhoneCallPlatform()
        self.platform = platform
        driver = VoiceCallDriver(platform: platform)
    }

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
        platform.account = PhoneCallPlatform.Account(id: accountId, voiceApi: voiceApi)
        // One that cannot hold keys dials without the claim.
        driver.start(agentId: agentId, keys: CallSealLine.supported ? keys : nil, device: device, language: language, fallback: fallback)
    }

    /// End call: `call_end` to the host, then close.
    func end() {
        driver.end()
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
        driver.toggleMute()
    }

    /// Speaker / Speaker off: the loudspeaker vs the earpiece (device-local,
    /// the host never hears of it).
    func toggleSpeaker() {
        driver.toggleSpeaker()
    }
}

/// The iOS side of `VoiceCallDriver`'s ports.
@MainActor
private final class PhoneCallPlatform: CallPlatform {
    struct Account {
        let id: String
        let voiceApi: VoiceApi
    }

    /// Who calls (set by `VoiceCallClient.start`).
    var account: Account?
    private let toaster = Toaster.shared
    private let ring = CallRingTone()

    private struct NoAccount: Error {}

    /// iOS holds nothing (no `audio` background mode): the call rings at once.
    func hold(speaker: Bool, answer: @escaping @MainActor (Bool, String?) -> Void) {
        CallAudioSession.setSpeaker(speaker)
        answer(true, nil)
    }

    func ringStart() { ring.start() }
    func ringStop() { ring.stop() }
    func setSpeaker(_ on: Bool) { CallAudioSession.setSpeaker(on) }
    func toast(_ text: String) { toaster.error(text) }

    func leaveAudioSession() {
        // On the audio queue like every enter (the ring's included).
        VoiceCallAudio.sharedQueue.async { CallAudioSession.leave() }
    }

    func requestMicrophone() async -> Bool {
        await AVAudioApplication.requestRecordPermission()
    }

    func startCall(agentId: String, seal: Bool) async throws -> VoiceCall.Ticket {
        guard let account else { throw NoAccount() }
        return try await account.voiceApi.startCall(accountId: account.id, agentId: agentId, seal: seal)
    }

    func refusal(_ error: Error) -> String { error.userFacingMessage }

    func openSocket(_ url: URL) -> CallSocket {
        WebSocketCall(URLSession.shared.webSocketTask(with: url))
    }

    func makeSeal(_ sealed: VoiceCallDriver.Sealed, io: CallSealLine.Io) -> CallSeal? {
        CallSealLine.make(callId: sealed.callId, agentId: sealed.agentId, deviceId: sealed.deviceId, target: sealed.target, keys: sealed.keys, io: io)
    }

    func makeAudio(
        onUplink: @escaping @Sendable (Data) -> Void,
        onLevel: @escaping @Sendable (Double, Bool) -> Void,
        onFailure: @escaping @Sendable (String) -> Void
    ) -> CallAudio {
        VoiceCallAudio(onUplink: onUplink, onLevel: onLevel, onFailure: onFailure)
    }

    func watchPath() -> CallPathWatch { CallLiveness() }

    func after(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer {
        TaskTimer(task: Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(ms))
            guard !Task.isCancelled else { return }
            body()
        })
    }

    func every(ms: Int, _ body: @escaping @MainActor () -> Void) -> CallTimer {
        TaskTimer(task: Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(ms))
                guard !Task.isCancelled else { return }
                body()
            }
        })
    }

    func keepAwake(_ awake: Bool) {
        UIApplication.shared.isIdleTimerDisabled = awake
    }
}

private struct TaskTimer: CallTimer {
    let task: Task<Void, Never>

    func cancel() { task.cancel() }
}

extension VoiceCallAudio: CallAudio {
    func open() async throws {
        do {
            try await start()
        } catch VoiceCallAudioError.noMicrophone {
            throw CallAudioError.noMicrophone
        }
    }
}

/// One call's WebSocket: the receive loop runs off the main actor and hands
/// every frame to the driver's `CallInbound`.
private final class WebSocketCall: CallSocket, @unchecked Sendable {
    private let task: URLSessionWebSocketTask
    private let uplink: UplinkSender

    init(_ task: URLSessionWebSocketTask) {
        self.task = task
        uplink = UplinkSender(task)
    }

    func send(text: String) {
        task.send(.string(text)) { _ in }
    }

    func sendLast(text: String) {
        let task = task
        task.send(.string(text)) { _ in task.cancel(with: .normalClosure, reason: nil) }
    }

    func send(binary: Data) {
        task.send(.data(binary)) { _ in }
    }

    func send(audio: Data) {
        uplink.send(audio)
    }

    func resume(_ inbound: CallInbound) {
        task.resume()
        Self.pump(task, inbound)
    }

    func close() {
        task.cancel(with: .normalClosure, reason: nil)
    }

    private static func pump(_ task: URLSessionWebSocketTask, _ inbound: CallInbound) {
        task.receive { result in
            switch result {
            case let .success(.data(frame)):
                inbound.data(frame)
            case let .success(.string(text)):
                inbound.text(text)
            case .success:
                inbound.other()
            case .failure:
                inbound.closed()
                return
            }
            pump(task, inbound)
        }
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
