import Foundation
import os

// #EXP-1941 (PR 11): the phone CALLER's half of a sealed call
// (fixtures/voice-call.json `seal`; the sealer is the eNet binding's
// `EnetCallHandshake`, the steps are `VoiceCall.sealStep`). The server marks
// a call sealed (`voice.startCall` answers `seal`); then this line sends the
// caller's hello after `call_ready`, takes the host's hello with its tag,
// resolves the chat's root and secret and finishes. ONE gate holds the
// microphone: `uplink` seals only through a sealer, which exists only after
// `finish` opened the host's tag (its proof that it holds the chat's secret
// and saw the same two hellos). Until then nothing of the microphone leaves,
// whatever the relay sends. Frames are opened by `inbound` in the socket's
// receive callback, in arrival order, before any hop to the main actor.
// Every seal failure reads `dropped`.

private let logger = Logger(subsystem: "at.exponential", category: "voice-call")

/// Where a caller gets the chat's key.
public protocol CallKeySource: Sendable {
    /// Waits (bounded) for a key holder that is still starting. False = this
    /// phone cannot hold keys: it dials without the claim, today's wire.
    func ready() async -> Bool
    /// `sessions.sendTarget {agentId}`: the chat's root, epoch, `rotationDue`.
    func target(agentId: String) async -> ServerSendTarget?
    /// The secret of (root, epoch): held, else ONE pull.
    func secret(rootId: String, epoch: UInt32) async -> Data?
    /// The highest epoch this install holds for the root.
    func highestEpoch(rootId: String) -> UInt32?
}

/// The app's key source: this install's key holder.
public struct HolderCallKeys: CallKeySource {
    let holder: TreeKeyHolder
    let accountId: String
    /// A host of this member advertises `call-seal`: only then is a holder
    /// that is still starting waited for.
    let sealable: Bool

    public init(holder: TreeKeyHolder, accountId: String, sealable: Bool) {
        self.holder = holder
        self.accountId = accountId
        self.sealable = sealable
    }

    public func ready() async -> Bool {
        await holder.readyInstall(accountId: accountId, wait: sealable ? sealHolderWait : .zero) != nil
    }

    public func target(agentId: String) async -> ServerSendTarget? {
        await holder.sendTarget(accountId: accountId, agentId: agentId, appId: nil)
    }

    public func secret(rootId: String, epoch: UInt32) async -> Data? {
        if let held = holder.held(rootId: rootId)[epoch] { return held }
        await holder.pull(accountId: accountId, rootId: rootId)
        return holder.held(rootId: rootId)[epoch]
    }

    public func highestEpoch(rootId: String) -> UInt32? {
        holder.held(rootId: rootId).keys.max()
    }
}

/// The binding's sealer, as the line uses it.
protocol CallSealer: Sendable {
    /// A packet above the step leaves as an empty frame with its seq (`replaced`): never a second size.
    func seal(seq: UInt16, flags: UInt8, packetOrEmpty packet: Data) throws -> (frame: Data, replaced: Bool)
    func open(audio frame: Data) throws -> (seq: UInt16, packet: Data)
    func openControl(data: String) throws -> String
}

public final class CallSealLine: @unchecked Sendable {
    public struct Io: Sendable {
        let sendBinary: @Sendable (Data) -> Void
        /// `call_end {reason}`.
        let end: @Sendable (String) -> Void
        /// The call is over with this toast code.
        let fail: @Sendable (String) -> Void

        public init(sendBinary: @escaping @Sendable (Data) -> Void, end: @escaping @Sendable (String) -> Void, fail: @escaping @Sendable (String) -> Void) {
            self.sendBinary = sendBinary
            self.end = end
            self.fail = fail
        }
    }

    typealias Finish = @Sendable (_ secret: Data, _ hostHello: Data, _ rootId: String, _ epoch: UInt32) throws -> CallSealer

    /// This build links the binding (`ENET_NODE`): it can seal a call.
    public static var supported: Bool {
        #if ENET_NODE
        true
        #else
        false
        #endif
    }

    /// The line of a call the server marked sealed; nil = this build cannot seal.
    /// `target` = the caller's `sendTarget` answer from before it dialled.
    public static func make(
        callId: String, agentId: String, deviceId: String, target: ServerSendTarget?, keys: CallKeySource, io: Io
    ) -> CallSealLine? {
        #if ENET_NODE
        let rootId = target?.rootId
        guard let handshake = try? EnetCallHandshake(role: .caller, rootId: rootId, epoch: rootId == nil ? 0 : target?.epoch ?? 0),
              let hello = try? handshake.hello()
        else { return nil }
        let finish: Finish = { secret, hostHello, rootId, epoch in
            try handshake.finish(input: EnetCallFinish(
                secret: secret, callId: callId, agentId: agentId, deviceId: deviceId,
                peerHello: hostHello, rootId: rootId, epoch: epoch, pad: callSealPad()
            ))
        }
        return CallSealLine(hello: hello, finish: finish, agentId: agentId, target: target, keys: keys, io: io)
        #else
        return nil
        #endif
    }

    private let lock = NSLock()
    private let hello: Data
    private let finishWith: Finish
    private let agentId: String
    private let keys: CallKeySource
    private let io: Io
    private var state = VoiceCall.SealState.idle
    private var target: ServerSendTarget?
    private var hostHello: (bytes: Data, rootId: String, epoch: UInt32)?
    private var secret: Data?
    private var sealer: CallSealer?
    private var isMuted = false
    /// Bumped by `close`: a key step that still blocks answers nobody.
    private var keyRun = 0
    private var helloTask: Task<Void, Never>?
    private var keyTask: Task<Void, Never>?
    /// Frames dropped (not well-formed, did not open) and packets replaced by an empty frame.
    private(set) var dropped = 0

    init(hello: Data, finish: @escaping Finish, agentId: String, target: ServerSendTarget?, keys: CallKeySource, io: Io) {
        self.hello = hello
        finishWith = finish
        self.agentId = agentId
        self.target = target
        self.keys = keys
        self.io = io
        step(.start(seal: true, required: nil))
    }

    /// True once `finish` opened the host's tag: the microphone may be sealed.
    public var sealed: Bool { lock.withLock { sealer != nil && state.phase == .sealed } }

    /// Mute rides inside every sealed frame (no `call_mute`).
    public var muted: Bool {
        get { lock.withLock { isMuted } }
        set { lock.withLock { isMuted = newValue } }
    }

    /// A text frame off the socket, in arrival order: the text the control
    /// handler reads (the relay's own frames, a clear `failed`, a host event
    /// opened from `sealed`), nil when the line took it.
    public func inbound(text: String) -> String? {
        guard let frame = Self.json(text) else { return text }
        switch frame["t"] as? String {
        case "call_ready":
            step(.ready)
            return text
        case "call_event":
            guard let event = frame["event"] as? [String: Any], event["t"] as? String != "failed" else { return text }
            let data = event["data"] as? String ?? ""
            switch event["t"] as? String {
            case "hello":
                let hello = Self.hostHello(data)
                lock.withLock { if hello != nil, state.phase == .offered { hostHello = hello } }
                step(.hello(ok: hello != nil))
                return nil
            case "sealed":
                let opened = lock.withLock { sealer }.flatMap { try? $0.openControl(data: data) }
                step(.sealed(opens: opened != nil))
                guard let opened, Self.json(opened)?["t"] as? String == "call_event" else { return nil }
                return opened
            default:
                step(.clear)
                return nil
            }
        default:
            return text
        }
    }

    /// A binary frame off the socket: the opened packet as today's down
    /// frame for the player, or nil (dropped, no packet, or the call ended).
    public func inbound(frame: Data) -> Data? {
        let opened = lock.withLock { sealer }.flatMap { try? $0.open(audio: frame) }
        step(.frame(opens: opened != nil))
        guard let opened, !opened.packet.isEmpty else { return nil }
        return VoiceCall.Wire.frame(kind: VoiceCall.Wire.kindDown, seq: opened.seq, packet: opened.packet)
    }

    /// THE capture path's gate: today's up frame (`[kind, seq, packet]`) as
    /// ONE sealed frame, mute inside; nil before the host's proof.
    public func uplink(_ clear: Data) -> Data? {
        guard let parsed = VoiceCall.Wire.parse(clear),
              let (sealer, muted) = lock.withLock({ state.phase == .sealed ? sealer.map { ($0, isMuted) } : nil })
        else { return nil }
        do {
            let sealed = try sealer.seal(seq: parsed.seq, flags: muted ? 1 : 0, packetOrEmpty: parsed.packet)
            if sealed.replaced { lock.withLock { dropped += 1 } }
            return sealed.frame
        } catch {
            step(.sealed(opens: false))
            return nil
        }
    }

    /// The call is over: the hello stops, the key source is abandoned.
    public func close() {
        let tasks = lock.withLock {
            keyRun += 1
            state = VoiceCall.SealState(phase: .over)
            sealer = nil
            secret = nil
            return [helloTask, keyTask]
        }
        tasks.forEach { $0?.cancel() }
    }

    private func step(_ event: VoiceCall.SealEvent) {
        let actions = lock.withLock {
            let stepped = VoiceCall.sealStep(state, event)
            state = stepped.state
            return stepped.actions
        }
        for action in actions { act(action) }
    }

    private func act(_ action: String) {
        let parts = action.split(separator: ":", maxSplits: 1).map(String.init)
        switch parts[0] {
        case "timer":
            DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(VoiceCall.Seal.handshakeMs)) { [weak self] in
                self?.step(.timeout(handshake: true))
            }
        case "hello":
            let frame = Data([VoiceCall.Wire.kindUp, VoiceCall.Seal.typeHello]) + hello
            io.sendBinary(frame)
            let task = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(VoiceCall.Seal.helloEveryMs))
                    guard let self, !Task.isCancelled, self.lock.withLock({ self.state.phase == .offered }) else { return }
                    self.io.sendBinary(frame)
                }
            }
            lock.withLock { helloTask = task }
        case "helloStop":
            lock.withLock { helloTask }?.cancel()
        case "key":
            let run = lock.withLock {
                keyRun += 1
                return keyRun
            }
            let task = Task { [weak self] in _ = await self?.resolveKey(run) }
            lock.withLock { keyTask = task }
        case "finish":
            finish()
        case "count":
            lock.withLock { dropped += 1 }
        case "end":
            io.end(parts.count > 1 ? parts[1] : "")
        case "fail":
            io.fail(parts.count > 1 ? parts[1] : VoiceCall.HangUpRules.failedCode)
        case "abandon":
            close()
        default:
            break
        }
    }

    /// The host's hello was taken: its root and epoch against the caller's
    /// own answer (asked again once), then the secret, pulled until it is there.
    private func resolveKey(_ run: Int) async {
        let live = { self.lock.withLock { run == self.keyRun } }
        guard let named = lock.withLock({ hostHello }) else { return step(.noKey) }
        let agrees = { (target: ServerSendTarget?) in
            target?.rootId?.lowercased() == named.rootId && target?.epoch == named.epoch && target?.rotationDue != true
        }
        if !agrees(lock.withLock({ target })) {
            let again = await keys.target(agentId: agentId)
            guard live() else { return }
            lock.withLock { target = again }
            if !agrees(again) { return step(.noKey) }
        }
        // No rollback: never an epoch below the highest this install holds.
        if let highest = keys.highestEpoch(rootId: named.rootId), named.epoch < highest {
            logger.warning("epoch_below_held root=\(named.rootId, privacy: .public) named=\(named.epoch) held=\(highest)")
            return step(.noKey)
        }
        while true {
            let secret = await keys.secret(rootId: named.rootId, epoch: named.epoch)
            guard live() else { return }
            if let secret {
                lock.withLock { self.secret = secret }
                return step(.key)
            }
            try? await Task.sleep(for: .milliseconds(VoiceCall.Seal.pullEveryMs))
            guard live() else { return }
        }
    }

    /// Once. The binding refuses a hello that names another root or epoch
    /// than the caller's own answer and opens the tag; a refusal used it up.
    private func finish() {
        let (secret, hello, target) = lock.withLock { (self.secret, hostHello, self.target) }
        var made: CallSealer?
        if let secret, let hello, let rootId = target?.rootId {
            do {
                made = try finishWith(secret, hello.bytes, rootId, target?.epoch ?? 0)
            } catch {
                logger.warning("the call handshake failed: \(String(describing: error), privacy: .public)")
            }
        }
        lock.withLock {
            sealer = made
            self.secret = nil
        }
        step(.finished(ok: made != nil))
    }

    private static func json(_ text: String) -> [String: Any]? {
        (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any]
    }

    /// A well-formed host hello (length, version, role) off its base64url text.
    static func hostHello(_ text: String) -> (bytes: Data, rootId: String, epoch: UInt32)? {
        var base64 = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        base64 += String(repeating: "=", count: (4 - base64.count % 4) % 4)
        guard let bytes = Data(base64Encoded: base64), bytes.count == VoiceCall.Seal.hostHelloBytes else { return nil }
        let b = [UInt8](bytes)
        guard b[0] == VoiceCall.Seal.version, b[1] == VoiceCall.Seal.roleHost else { return nil }
        let epoch = b[2..<6].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
        var hex = b[6..<22].map { String(format: "%02x", $0) }.joined()
        for index in [20, 16, 12, 8] { hex.insert("-", at: hex.index(hex.startIndex, offsetBy: index)) }
        return (bytes, hex, epoch)
    }
}

#if ENET_NODE
extension EnetCallSeal: CallSealer {
    func seal(seq: UInt16, flags: UInt8, packetOrEmpty packet: Data) throws -> (frame: Data, replaced: Bool) {
        do {
            return (try sealAudio(seq: seq, flags: flags, packet: packet), false)
        } catch EnetError.Crypto(let reason) where reason.hasSuffix("too_large") {
            return (try sealAudio(seq: seq, flags: flags, packet: Data()), true)
        }
    }

    func open(audio frame: Data) throws -> (seq: UInt16, packet: Data) {
        let opened = try openAudio(frame: frame)
        return (opened.seq, opened.packet)
    }
}
#endif
