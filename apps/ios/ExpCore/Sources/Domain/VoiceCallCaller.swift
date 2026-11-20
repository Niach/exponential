import Foundation

// #EXP-1993: the phone caller's session as one pure machine
// (fixtures/voice-call.json `caller`, whose `$comment` is the spec; Android
// replays the same cases). `VoiceCallClient` only does the effects, in
// order, and hands in what happened.

public extension VoiceCall {
    enum Caller {
        /// Where the session is: the tap went (`holding`), it rings and asks
        /// the ticket (`asking`), the ticket's url was dialled (`dialed`),
        /// `call_ready` came (`paired`).
        public enum Stage: String, Sendable { case idle, holding, asking, dialed, paired }

        public struct State: Equatable, Sendable {
            /// The flow (`callStep`).
            public var call: VoiceCall.State
            public var stage: Stage
            /// The ticket's `seal`: mute rides inside every sealed frame.
            public var sealed: Bool

            public init(call: VoiceCall.State, stage: Stage, sealed: Bool = false) {
                self.call = call
                self.stage = stage
                self.sealed = sealed
            }

            public static let idle = State(call: VoiceCall.idle, stage: .idle)
        }

        /// One control frame off the socket (`wire.control` and the relay's
        /// `error` / `bye`), a sealed call's opened by its line first.
        public enum Frame: Equatable, Sendable {
            case ready
            /// nil = an event this build does not know.
            case event(Event?)
            case end(reason: String?)
            case error(code: String?)
            case bye(outcome: String?)
            case alive
            case other

            public init(json: [String: Any]) {
                switch json["t"] as? String {
                case "call_ready": self = .ready
                case "call_event": self = .event((json["event"] as? [String: Any]).flatMap(Event.init(json:)))
                case "call_end": self = .end(reason: json["reason"] as? String)
                case "error": self = .error(code: json["code"] as? String)
                case "bye": self = .bye(outcome: json["outcome"] as? String)
                case "call_alive": self = .alive
                default: self = .other
                }
            }

            /// nil = not a JSON object.
            public init?(text: String) {
                guard let json = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] else { return nil }
                self.init(json: json)
            }
        }

        /// What happened (the fixture's `t`).
        public enum Input: Equatable, Sendable {
            /// The tap, on the device `callTarget` named.
            case start(device: String, language: String?, fallback: Bool)
            /// The hold's answer; a refusal fails the call with `code` (nil = dropped).
            case held(ok: Bool, code: String?)
            /// `voice.startCall`'s usable answer (EXP-1857: its device and fallback win).
            case ticket(device: String?, fallback: Bool?, sealed: Bool)
            /// Its refusal, in this platform's words.
            case refused(message: String)
            case frame(Frame)
            /// The socket went away.
            case closed
            /// The call's own failure; `told` = the seal line sent its own `call_end`.
            case failed(code: String, told: Bool)
            /// The ring's bound (`ring`'s `ms`) passed.
            case ringTimeout
            case mute
            case speaker
            /// End call (also an account switch, the app leaving the foreground).
            case end
        }

        /// A control frame the caller sends.
        public enum Outgoing: Equatable, Sendable {
            case end(reason: String?)
            case mute(Bool)

            public var json: [String: Any] {
                switch self {
                case let .end(reason?): return ["t": "call_end", "reason": reason]
                case .end(nil): return ["t": "call_end"]
                case let .mute(muted): return ["t": "call_mute", "muted": muted]
                }
            }

            /// The frame as the socket's text.
            public var text: String {
                let data = (try? JSONSerialization.data(withJSONObject: json, options: [.sortedKeys])) ?? Data()
                return String(decoding: data, as: UTF8.self)
            }
        }

        /// What to do (the fixture's `do`), in order.
        public enum Effect: Equatable, Sendable {
            case hold
            /// The ring tone, looped; `ringTimeout` after `ms`.
            case ring(ms: Int)
            case ticket
            case dial
            case startAudio
            /// Answered: the ring stops, the timer starts.
            case stopRing
            /// Barge-in: the buffered reply goes.
            case flush
            case mute(Bool)
            case speaker(Bool)
            /// On the call's socket (nothing while it still connects).
            case send(Outgoing)
            /// `call_alive`: the liveness deadline starts.
            case alive
            /// Everything the call holds goes.
            case release
            case toast(String)
        }

        public static func step(_ state: State, _ input: Input) -> (state: State, effects: [Effect]) {
            let stay = (state: state, effects: [Effect]())
            if case let .start(device, language, fallback) = input {
                guard state.stage == .idle else { return stay }
                let call = callStep(state.call, .start(device: device, language: language, fallback: fallback))
                return (State(call: call, stage: .holding), [.hold])
            }
            guard state.stage != .idle else { return stay }
            let dialled = state.stage == .dialed || state.stage == .paired
            // A call ends once: the flow steps `event`, everything goes, then its toast.
            func ended(_ event: Event, first: [Effect] = []) -> (state: State, effects: [Effect]) {
                let call = callStep(state.call, event)
                return (State(call: call, stage: .idle), first + [.release] + (call.toast.map { [.toast($0)] } ?? []))
            }
            // The host learns an own failure from the caller, once dialled.
            func own(_ code: String, told: Bool = false) -> (state: State, effects: [Effect]) {
                ended(.failed(code: code), first: dialled && !told ? [.send(.end(reason: HangUpRules.failedCode))] : [])
            }
            func wire(_ code: String?) -> String {
                guard let code, !code.isEmpty else { return HangUpRules.failedCode }
                return Seal.wireCode(code)
            }
            var next = state
            switch input {
            case .start:
                return stay
            case let .held(ok, code):
                guard state.stage == .holding else { return stay }
                guard ok else { return own(code ?? HangUpRules.failedCode) }
                next.stage = .asking
                return (next, [.ring(ms: Constants.maxRingMs), .ticket])
            case let .ticket(device, fallback, sealed):
                guard state.stage == .asking else { return stay }
                next.stage = .dialed
                next.sealed = sealed
                if let device { next.call.device = device }
                if let fallback { next.call.fallback = fallback }
                return (next, [.dial])
            case let .refused(message):
                guard state.stage == .asking else { return stay }
                return (.idle, [.release, .toast(message)])
            case let .frame(frame):
                guard dialled else { return stay }
                switch frame {
                case .ready:
                    guard state.stage == .dialed else { return stay }
                    next.stage = .paired
                    return (next, [.startAudio])
                case let .event(event):
                    switch event {
                    case nil, .start, .mute, .unmute, .speaker:
                        return stay
                    // #EXP-1936: as a `call_end` would.
                    case .end:
                        return ended(.hungUp(reason: nil))
                    case let .hungUp(reason):
                        return ended(.hungUp(reason: reason))
                    // The host failed the call (its `call_end` follows): nothing goes back.
                    case let .failed(code):
                        return ended(.failed(code: wire(code)))
                    case let event?:
                        next.call = callStep(state.call, event)
                        var effects: [Effect] = event == .speechStarted ? [.flush] : []
                        if state.call.phase == .ringing, next.call.phase != .ringing { effects.append(.stopRing) }
                        return (next, effects)
                    }
                case let .end(reason):
                    return ended(.hungUp(reason: reason))
                case let .error(code):
                    return ended(.failed(code: wire(code)))
                case let .bye(outcome):
                    return ended(.failed(code: wire(outcome)))
                case .alive:
                    return (state, [.alive])
                case .other:
                    return stay
                }
            case .closed:
                return dialled ? ended(.failed(code: HangUpRules.failedCode)) : stay
            case let .failed(code, told):
                return own(code, told: told)
            case .ringTimeout:
                return state.call.phase == .ringing ? own("unavailable") : stay
            case .mute:
                let muted = !state.call.muted
                next.call = callStep(state.call, muted ? .mute : .unmute)
                // A sealed call sends no `call_mute`: mute rides inside every sealed frame.
                return (next, [.mute(muted)] + (state.stage == .paired && !state.sealed ? [.send(.mute(muted))] : []))
            case .speaker:
                let on = !state.call.speaker
                next.call = callStep(state.call, .speaker(on: on))
                return (next, [.speaker(on)])
            case .end:
                return ended(.end, first: dialled ? [.send(.end(reason: nil))] : [])
            }
        }
    }
}
