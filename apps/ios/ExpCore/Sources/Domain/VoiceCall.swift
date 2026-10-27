import Foundation

// NEW-69 (voice V5): CALLER MODE on the phones. A port of the TS reference
// `packages/domain-contract/src/voice-call.ts` (desktop `domain::voice_call`,
// Android `domain/VoiceCall.kt`), byte-locked by `fixtures/voice-call.json`
// (`copy`, `testIds`, `icons`, `constants`, `wire`, `surfaceSpecimens`, every
// `availabilityCases` and `flowCases` row, replayed by `VoiceCallTests`). The
// fixture's `$comment` holds the rules: a phone STREAMS the call to the
// member's voice device, which recognises and speaks; the phone folds the
// host's `call_event` frames into the same `callStep` machine.

public enum VoiceCall {
    public enum Copy {
        public static let call = "Call {agent}"
        public static let connecting = "Connecting"
        public static let listening = "Listening"
        public static let thinking = "Thinking"
        public static let speaking = "Speaking"
        public static let muted = "Microphone muted"
        public static let mutedLine = "Unmute to talk"
        public static let mute = "Mute"
        public static let unmute = "Unmute"
        public static let endCall = "End call"
        public static let queued = "{count} queued · {text}"

        /// Why the telephone is disabled (`callAvailability`, in order).
        public enum Reasons {
            public static let noVoiceDevice = "Pick a voice device in Settings → Voice"
            public static let speakOff = "Turn on Speak replies in Settings → Voice"
            public static let offline = "{device} is offline"
            public static let notHere = "Calls run on {device}"
            public static let preparing = "{device} is getting its voice ready"
            public static let noGpu = "{device} can't speak: no usable GPU"
            public static let busy = "{device} is in another call"
        }

        /// The toast a failed call ends with, by error code; an unknown code
        /// reads as `dropped`.
        public static let failed: [String: String] = [
            "no_answer": "{device} didn't answer",
            "device_offline": "{device} is offline",
            "no_microphone": "No microphone",
            "not_granted": "Microphone not allowed",
            "dropped": "The call dropped",
            "busy": "{device} is in another call",
            "unsupported": "This browser can't make calls",
        ]

        /// `call` for one agent (the telephone's label).
        public static func call(agent: String) -> String {
            call.replacingOccurrences(of: "{agent}", with: agent)
        }
    }

    public enum TestIds {
        public static let call = "agent-chat-call"
        public static let surface = "voice-call"
        public static let mute = "voice-call-mute"
        public static let end = "voice-call-end"
    }

    public enum Icons {
        /// The telephone's icon concept (`AppIcons.voiceCall`).
        public static let call = "voice-call"
    }

    public enum Constants {
        public static let ringTimeoutMs = 10_000
        public static let minSilenceMs = 250
        public static let bargeInMs = 250
        public static let frameMs = 20
        public static let upRate = 16_000
        public static let downRate = 24_000
        public static let upKbps = 24
        public static let downKbps = 32
        public static let jitterMs = 40
        public static let maxFramesPerSecond = 60
        public static let maxCallMs = 3_600_000
        public static let idleTimeoutMs = 30_000
    }

    /// `hangUpRules`: a `call_end` steps `hungUp(reason)`; ANY reason
    /// (`idle`, `max_length`, `dropped`, or one this build doesn't know)
    /// fails the call as `failedCode`, only a reason-less one ends silently.
    public enum HangUpRules {
        public static let failedCode = "dropped"
    }

    /// #NEW-89: the phone's audio session around a call (iOS).
    public enum AudioSessionRules {
        /// An AVAudioSession interruption drops the call only when it BEGAN
        /// now: a `.began` carrying `wasSuspended` reports the app's earlier
        /// suspension (delivered late, on resume), not another call.
        public static func interruptionDropsCall(began: Bool, wasSuspended: Bool) -> Bool {
            began && !wasSuspended
        }

        /// What the capture chain (the tap and its resampler) does when the
        /// engine's configuration changes (a route change: AirPods connect
        /// or disconnect mid-call). A different input format rebuilds it; a
        /// route without an input (rate or channels 0) drops the call.
        public enum CaptureChange: Equatable, Sendable { case keep, rebuild, drop }

        public struct InputFormat: Equatable, Sendable {
            public let sampleRate: Double
            public let channels: Int

            public init(sampleRate: Double, channels: Int) {
                self.sampleRate = sampleRate
                self.channels = channels
            }

            public var usable: Bool { sampleRate > 0 && channels > 0 }
        }

        public static func captureChange(built: InputFormat?, route: InputFormat) -> CaptureChange {
            guard route.usable else { return .drop }
            return built == route ? .keep : .rebuild
        }

        /// The tap's buffer size: one `frameMs` frame at the input's rate.
        public static func tapFrames(sampleRate: Double) -> Int {
            Int(sampleRate * Double(Constants.frameMs) / 1000)
        }
    }

    /// Cuts the captured `upRate` samples into `frameMs` frames for the
    /// encoder. While muted every frame is SILENCE, never nothing: the relay
    /// ends a call after `idleTimeoutMs` without a frame and the host only
    /// talks back while speaking, so a muted caller keeps the call alive
    /// with encoded silence.
    public struct Uplink: Sendable {
        public static let frameSamples = Constants.upRate * Constants.frameMs / 1000

        public private(set) var muted = false
        private var pending: [Int16] = []

        public init() {}

        /// Mute / unmute drops the partial frame.
        public mutating func setMuted(_ muted: Bool) {
            self.muted = muted
            pending = []
        }

        /// Drops the partial frame (a restart); `muted` stays.
        public mutating func reset() {
            pending = []
        }

        /// The whole frames `samples` completes, in order.
        public mutating func push<S: Collection>(_ samples: S) -> [[Int16]] where S.Element == Int16 {
            if muted {
                pending.append(contentsOf: repeatElement(0, count: samples.count))
            } else {
                pending.append(contentsOf: samples)
            }
            var frames: [[Int16]] = []
            while pending.count >= Self.frameSamples {
                frames.append(Array(pending.prefix(Self.frameSamples)))
                pending.removeFirst(Self.frameSamples)
            }
            return frames
        }
    }

    /// The uplink's send queue: ONE frame in flight on the socket, at most
    /// `capacity` waiting behind it; a stalled network drops the OLDEST
    /// waiting frames, so the queue never grows and the call stays live.
    public struct Outbox<Frame: Sendable>: Sendable {
        /// 500 ms of frames.
        public static var capacity: Int { 500 / Constants.frameMs }

        public private(set) var waiting: [Frame] = []
        public private(set) var inFlight = false
        public private(set) var dropped = 0

        public init() {}

        /// Offers a frame; returns it when it goes out now (nothing in flight).
        public mutating func offer(_ frame: Frame) -> Frame? {
            guard inFlight else {
                inFlight = true
                return frame
            }
            waiting.append(frame)
            if waiting.count > Self.capacity {
                waiting.removeFirst(waiting.count - Self.capacity)
                dropped += 1
            }
            return nil
        }

        /// The in-flight frame was sent; returns the next one to send.
        public mutating func sent() -> Frame? {
            guard !waiting.isEmpty else {
                inFlight = false
                return nil
            }
            return waiting.removeFirst()
        }
    }

    /// The binary audio frame: byte 0 = kind, bytes 1-2 = the u16 big-endian
    /// sequence, then ONE Opus packet.
    public enum Wire {
        public static let kindUp: UInt8 = 1
        public static let kindDown: UInt8 = 2
        public static let maxFrameBytes = 1500
        public static let headerBytes = 3

        /// One audio frame.
        public static func frame(kind: UInt8, seq: UInt16, packet: Data) -> Data {
            var out = Data([kind, UInt8(seq >> 8), UInt8(seq & 0xff)])
            out.append(packet)
            return out
        }

        /// `(kind, seq, packet)` of a well-formed frame within the size cap.
        public static func parse(_ frame: Data) -> (kind: UInt8, seq: UInt16, packet: Data)? {
            guard frame.count > headerBytes, frame.count <= maxFrameBytes else { return nil }
            let bytes = [UInt8](frame.prefix(headerBytes))
            return (bytes[0], UInt16(bytes[1]) << 8 | UInt16(bytes[2]), Data(frame.dropFirst(headerBytes)))
        }
    }

    // MARK: - Availability

    public enum Platform: String, Sendable { case desktop, phone, web }

    /// The voice device's doctor row `speech` as a caller reads it.
    public enum Speech: String, Sendable { case ok, preparing, noGpu }

    public struct AvailabilityInput: Equatable, Sendable {
        public var platform: Platform
        /// The voice device's name; nil = none picked (or its row is gone).
        public var device: String?
        public var speakReplies: Bool
        public var online: Bool
        /// THIS client is the voice device.
        public var here: Bool
        /// nil = no `speech` row yet.
        public var speech: Speech?
        /// A call already runs on the voice device.
        public var busy: Bool

        public init(
            platform: Platform, device: String?, speakReplies: Bool, online: Bool,
            here: Bool, speech: Speech?, busy: Bool
        ) {
            self.platform = platform
            self.device = device
            self.speakReplies = speakReplies
            self.online = online
            self.here = here
            self.speech = speech
            self.busy = busy
        }
    }

    public struct Availability: Equatable, Sendable {
        public var enabled: Bool
        public var reason: String?

        public init(enabled: Bool, reason: String?) {
            self.enabled = enabled
            self.reason = reason
        }
    }

    /// Whether the telephone is enabled, and the reason when it is not
    /// (first match wins).
    public static func callAvailability(_ input: AvailabilityInput) -> Availability {
        func off(_ reason: String) -> Availability {
            Availability(enabled: false, reason: reason.replacingOccurrences(of: "{device}", with: input.device ?? ""))
        }
        guard input.device != nil else { return off(Copy.Reasons.noVoiceDevice) }
        if !input.speakReplies { return off(Copy.Reasons.speakOff) }
        if !input.online { return off(Copy.Reasons.offline) }
        if input.platform == .desktop && !input.here { return off(Copy.Reasons.notHere) }
        if input.speech == nil || input.speech == .preparing { return off(Copy.Reasons.preparing) }
        if input.speech == .noGpu { return off(Copy.Reasons.noGpu) }
        if input.busy { return off(Copy.Reasons.busy) }
        return Availability(enabled: true, reason: nil)
    }

    /// The doctor's `speech` row (group `voice`): ok → ok, missing (no
    /// usable GPU) → noGpu, absent → nil, anything else → preparing (web
    /// `callSpeechOf`).
    public static func callSpeech(of doctor: DeviceDoctor?) -> Speech? {
        guard let row = doctor?.items.first(where: { $0.key == "speech" && $0.group == "voice" }) else { return nil }
        switch row.state {
        case "ok": return .ok
        case "missing": return .noGpu
        default: return .preparing
        }
    }

    /// `callSpeech(of:)` over the synced `devices.doctor` column (stringified
    /// JSON; NULL or undecodable = no report).
    public static func callSpeech(ofJSON json: String?) -> Speech? {
        callSpeech(of: DeviceDoctor.decode(json: json))
    }

    // MARK: - Flow

    public enum Phase: String, Sendable { case hidden, connecting, listening, thinking, speaking }

    public struct State: Equatable, Sendable {
        public var phase: Phase
        public var muted: Bool
        public var device: String
        /// The last utterance, quoted (what Thinking shows).
        public var said: String
        public var line: String
        public var toast: String?

        public init(phase: Phase, muted: Bool, device: String, said: String, line: String, toast: String?) {
            self.phase = phase
            self.muted = muted
            self.device = device
            self.said = said
            self.line = line
            self.toast = toast
        }
    }

    /// One flow event (`flowCases`; the host sends partial, said, queued,
    /// replyStarted, replyFinished and speechStarted as `call_event`).
    public enum Event: Equatable, Sendable {
        case start(device: String)
        case connected
        case partial(text: String)
        case said(text: String)
        case speechStarted
        case queued(count: Int, text: String)
        case replyStarted(text: String)
        case replyFinished
        case mute
        case unmute
        case end
        case hungUp(reason: String?)
        case failed(code: String)

        /// The event a JSON object names by its `t`; nil for an unknown `t`.
        public init?(json: [String: Any]) {
            let text = json["text"] as? String ?? ""
            switch json["t"] as? String {
            case "start": self = .start(device: json["device"] as? String ?? "")
            case "connected": self = .connected
            case "partial": self = .partial(text: text)
            case "said": self = .said(text: text)
            case "speechStarted": self = .speechStarted
            case "queued": self = .queued(count: (json["count"] as? NSNumber)?.intValue ?? 0, text: text)
            case "replyStarted": self = .replyStarted(text: text)
            case "replyFinished": self = .replyFinished
            case "mute": self = .mute
            case "unmute": self = .unmute
            case "end": self = .end
            case "hungUp": self = .hungUp(reason: json["reason"] as? String)
            case "failed": self = .failed(code: json["code"] as? String ?? "")
            default: return nil
            }
        }

        /// The events a HOST may send in a `call_event` frame; the caller
        /// owns start, connected, mute, unmute, end, hungUp and failed itself.
        public var isHostEvent: Bool {
            switch self {
            case .partial, .said, .queued, .replyStarted, .replyFinished, .speechStarted: true
            default: false
            }
        }
    }

    /// No call (`CALL_IDLE`).
    public static let idle = State(phase: .hidden, muted: false, device: "", said: "", line: "", toast: nil)

    private static func quoted(_ text: String) -> String { "“\(text)”" }

    private static func trimmed(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// One step of the call (the same machine on the voice device and a caller).
    public static func callStep(_ state: State, _ event: Event) -> State {
        func at(_ phase: Phase, _ line: String, said: String? = nil) -> State {
            var next = state
            next.phase = phase
            next.line = line
            next.toast = nil
            if let said { next.said = said }
            return next
        }
        if case let .start(device) = event {
            var next = idle
            next.phase = .connecting
            next.device = device
            return next
        }
        if state.phase == .hidden { return state }
        switch event {
        case .start:
            return state
        case .connected:
            return state.phase == .connecting ? at(.listening, "") : state
        case let .partial(text):
            let text = trimmed(text)
            return state.phase == .listening && !text.isEmpty ? at(.listening, quoted(text)) : state
        case let .said(text):
            let text = trimmed(text)
            guard !text.isEmpty else { return state }
            return at(.thinking, quoted(text), said: quoted(text))
        case .speechStarted:
            return state.phase == .speaking ? at(.listening, "") : state
        case let .queued(count, text):
            guard state.phase == .thinking || state.phase == .speaking else { return state }
            if count <= 0 { return at(state.phase, state.phase == .thinking ? state.said : state.line) }
            return at(
                state.phase,
                Copy.queued
                    .replacingOccurrences(of: "{count}", with: String(count))
                    .replacingOccurrences(of: "{text}", with: quoted(trimmed(text)))
            )
        case let .replyStarted(text):
            return at(.speaking, trimmed(text))
        case .replyFinished:
            return state.phase == .speaking ? at(.listening, "") : state
        case .mute:
            var next = state
            next.muted = true
            return next
        case .unmute:
            var next = state
            next.muted = false
            return next
        case .end:
            return idle
        case let .hungUp(reason):
            return (reason ?? "").isEmpty ? idle : callStep(state, .failed(code: HangUpRules.failedCode))
        case let .failed(code):
            var next = idle
            next.toast = (Copy.failed[code] ?? Copy.failed["dropped"] ?? "")
                .replacingOccurrences(of: "{device}", with: state.device)
            return next
        }
    }

    /// The call surface's activity (the VoiceWave activities it uses).
    public enum Activity: String, Sendable { case listening, thinking, speaking, muted }

    public struct View: Equatable, Sendable {
        public var activity: Activity
        public var state: String
        public var line: String
        /// The mute pill's label (Mute / Unmute); End call is always beside it.
        public var mute: String

        public init(activity: Activity, state: String, line: String, mute: String) {
            self.activity = activity
            self.state = state
            self.line = line
            self.mute = mute
        }
    }

    /// What the call surface shows; nil = no surface.
    public static func callView(_ state: State) -> View? {
        let mute = state.muted ? Copy.unmute : Copy.mute
        switch state.phase {
        case .hidden:
            return nil
        case .connecting:
            return View(activity: .thinking, state: Copy.connecting, line: state.line, mute: mute)
        case .listening:
            return state.muted
                ? View(activity: .muted, state: Copy.muted, line: Copy.mutedLine, mute: mute)
                : View(activity: .listening, state: Copy.listening, line: state.line, mute: mute)
        case .thinking:
            return View(activity: .thinking, state: Copy.thinking, line: state.line, mute: mute)
        case .speaking:
            return View(activity: .speaking, state: Copy.speaking, line: state.line, mute: mute)
        }
    }

    // MARK: - Surface specimens

    /// One styleguide row of the call surface (`surfaceSpecimens`).
    public struct Specimen: Equatable, Sendable {
        public var activity: Activity
        public var level: Double
        public var bands: [Double]
        public var state: String
        public var line: String
        public var mute: String

        public init(activity: Activity, level: Double, bands: [Double], state: String, line: String, mute: String) {
            self.activity = activity
            self.level = level
            self.bands = bands
            self.state = state
            self.line = line
            self.mute = mute
        }

        /// The surface's view of this row.
        public var view: View { View(activity: activity, state: state, line: line, mute: mute) }
    }

    public static let surfaceSpecimens: [Specimen] = [
        Specimen(activity: .listening, level: 0.7, bands: [0.9, 0.6, 1, 0.5], state: "Listening", line: "“what is still open on the voice board”", mute: "Mute"),
        Specimen(activity: .thinking, level: 0, bands: [], state: "Thinking", line: "1 queued · “and assign them to me”", mute: "Mute"),
        Specimen(activity: .speaking, level: 0.5, bands: [0.7, 1, 0.5, 0.8], state: "Speaking", line: "Three issues are open: NEW-55, NEW-56 and NEW-79.", mute: "Mute"),
        Specimen(activity: .muted, level: 0, bands: [], state: "Microphone muted", line: "Unmute to talk", mute: "Unmute"),
    ]

    // MARK: - Transport

    /// `voice.startCall`'s answer: the caller's relay ticket and the URL to
    /// dial AS-IS.
    public struct Ticket: Decodable, Equatable, Sendable {
        public let callId: String
        public let ticket: String
        public let url: String

        public init(callId: String, ticket: String, url: String) {
            self.callId = callId
            self.ticket = ticket
            self.url = url
        }
    }
}
