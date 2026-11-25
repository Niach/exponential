import Foundation

// EXP-2111 (fixtures/voice-call.json `log`, whose `$comment` is the spec; web
// and Android replay the same cases): one call's log lines and, when it ended
// any other way than `quiet`, its one event. Never a word of the call, never
// an agent's or a device's name.

/// Where a call's lines and its failure event go (any thread).
public protocol CallLogSink: Sendable {
    /// A monotonic clock, in ms.
    func nowMs() -> Int
    func line(_ line: String)
    /// The call ended any other way than a hang-up.
    func failed(_ end: VoiceCall.CallLog.End)
}

public extension VoiceCall {
    final class CallLog: @unchecked Sendable {
        /// What only the caller's driver sees.
        public enum Note: Equatable, Sendable {
            case ticket(callId: String?, fallback: Bool, sealed: Bool)
            case socketOpen
            case up
            case down
            case silent(ms: Int)
            case dialFailed
            case noSeal
            case audioFailed
        }

        /// A call's end: who, why and how far it got.
        public struct End: Equatable, Sendable {
            public let tag: String
            public let by: String
            public let reason: String
            public let stage: String
            public let phase: String
            public let answered: Bool
            public let sealed: Bool
            public let lastedMs: Int
            public let upFrames: Int
            public let downFrames: Int
            /// Before the end; nil = the host was never heard.
            public let lastDownMs: Int?

            public var message: String { "Voice call ended: \(reason)" }
        }

        /// Ends that are no event: a hang-up, a refusal.
        public static let quiet: Set<String> = ["hung_up", "refused", "not_granted", "busy", "meeting"]

        private let sink: CallLogSink
        private let lock = NSLock()
        private var tag = "-"
        private var startedAt = 0
        private var answered = false
        private var over = false
        private var upFrames = 0
        private var downFrames = 0
        private var lastDownAt = 0

        public init(_ sink: CallLogSink) {
            self.sink = sink
        }

        /// One input of the machine, with the state before it and its answer.
        public func input(_ input: Caller.Input, before: Caller.State, after: Caller.State, effects: [Caller.Effect]) {
            var lines: [String] = []
            var end: End?
            lock.withLock {
                guard !over, after != before || !effects.isEmpty else { return }
                if case let .start(_, _, fallback) = input {
                    startedAt = sink.nowMs()
                    lines.append(text("started (fallback \(fallback))"))
                    return
                }
                if effects.contains(.release) {
                    let ended = ending(input, before)
                    lines.append(text(
                        "ended by=\(ended.by) reason=\(ended.reason) stage=\(ended.stage) phase=\(ended.phase) answered=\(ended.answered) "
                            + "lasted=\(ended.lastedMs) ms sealed=\(ended.sealed) up=\(ended.upFrames) down=\(ended.downFrames) "
                            + "last_down=\(ended.lastDownMs.map { "\($0) ms ago" } ?? "never")"
                    ))
                    if !Self.quiet.contains(ended.reason) { end = ended }
                    return
                }
                if let stage = stage(input) { lines.append(text(stage)) }
                if !answered, before.call.phase == .ringing, after.call.phase != .ringing {
                    answered = true
                    lines.append(text("answered after \(since()) ms"))
                }
            }
            lines.forEach(sink.line)
            if let end { sink.failed(end) }
        }

        public func note(_ note: Note) {
            let line: String? = lock.withLock {
                guard !over else { return nil }
                switch note {
                case let .ticket(callId, fallback, sealed):
                    if let callId, !callId.isEmpty { tag = String(callId.prefix(8)).lowercased() }
                    return text("ticket after \(since()) ms (fallback \(fallback), sealed \(sealed))")
                case .socketOpen:
                    return text("the relay socket is open after \(since()) ms")
                case .up:
                    upFrames += 1
                    return upFrames == 1 ? text("the first microphone frame went out after \(since()) ms") : nil
                case .down:
                    downFrames += 1
                    lastDownAt = sink.nowMs()
                    return downFrames == 1 ? text("the host's first audio frame came after \(since()) ms") : nil
                case let .silent(ms):
                    return text("the path went silent: nothing from the relay for \(ms) ms")
                case .dialFailed:
                    return text("the dial failed")
                case .noSeal:
                    return text("the call cannot be sealed here")
                case .audioFailed:
                    return text("the call audio did not start")
                }
            }
            if let line { sink.line(line) }
        }

        private func text(_ text: String) -> String { "call \(tag): \(text)" }

        private func since() -> Int { sink.nowMs() - startedAt }

        private func stage(_ input: Caller.Input) -> String? {
            switch input {
            case .held: "the microphone is held"
            case .frame(.ready): "call_ready: the host joined after \(since()) ms"
            case .frame(.event(.rang)): "the host rang"
            case .frame(.event(.ready)): "the host's voice is ready"
            case .frame(.event(.noSound)): "the host hears no sound from this microphone"
            default: nil
            }
        }

        private func ending(_ input: Caller.Input, _ before: Caller.State) -> End {
            over = true
            let (by, reason) = Self.who(input)
            return End(
                tag: tag, by: by, reason: reason, stage: before.stage.rawValue, phase: before.call.phase.rawValue,
                answered: answered, sealed: before.sealed, lastedMs: since(), upFrames: upFrames, downFrames: downFrames,
                lastDownMs: downFrames == 0 ? nil : sink.nowMs() - lastDownAt
            )
        }

        private static func code(_ code: String?) -> String {
            guard let code, !code.isEmpty else { return HangUpRules.failedCode }
            // `\z`: a `$` would let a trailing newline through.
            return code.range(of: #"^[A-Za-z][A-Za-z0-9_]{0,31}\z"#, options: .regularExpression) == nil ? "other" : code
        }

        /// Who ended the call and why, from the input that did.
        private static func who(_ input: Caller.Input) -> (by: String, reason: String) {
            func hungUp(_ reason: String?) -> (by: String, reason: String) { ("far", reason == nil ? "hung_up" : code(reason)) }
            switch input {
            case let .held(_, held): return ("here", code(held))
            case let .failed(failed, _): return ("here", code(failed))
            case .refused: return ("server", "refused")
            case .closed: return ("link", "socket_closed")
            case .ringTimeout: return ("here", "unavailable")
            case let .frame(.end(reason)): return hungUp(reason)
            case let .frame(.error(relay)): return ("relay", code(relay))
            case let .frame(.bye(outcome)): return ("relay", code(outcome))
            case let .frame(.event(.failed(failed))): return ("host", code(failed))
            case let .frame(.event(.hungUp(reason))): return hungUp(reason)
            case .frame: return hungUp(nil)
            default: return ("here", "hung_up")
            }
        }
    }
}
