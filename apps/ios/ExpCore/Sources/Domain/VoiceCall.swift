import Foundation

// NEW-69 (voice V5): CALLER MODE on the phones. A port of the TS reference
// `packages/etalk/src/voice-call.ts` (desktop `domain::voice_call`,
// Android `domain/VoiceCall.kt`), byte-locked by `fixtures/voice-call.json`
// (`copy`, `testIds`, `icons`, `constants`, `wire`, `surfaceSpecimens`, every
// `availabilityCases` and `flowCases` row, replayed by `VoiceCallTests`). The
// fixture's `$comment` holds the rules: a phone STREAMS the call to the
// member's voice device, which recognises and speaks; the phone folds the
// host's `call_event` frames into the same `callStep` machine.

public enum VoiceCall {
    public enum Copy {
        public static let call = "Call {agent}"
        public static let calling = "Calling…"
        public static let gettingReady = "Getting ready…"
        public static let noSound = "Can't hear your microphone"
        public static let listening = "Listening"
        public static let thinking = "Thinking"
        public static let speaking = "Speaking"
        public static let muted = "Microphone muted"
        public static let mute = "Mute"
        public static let unmute = "Unmute"
        public static let speaker = "Speaker"
        public static let speakerOff = "Speaker off"
        public static let endCall = "End call"
        public static let cancel = "Cancel"
        public static let queued = "{count} queued · {text}"
        /// S-3: what the voice device says once the call is answered.
        public static let greeting: [String: String] = ["en": "Hello?", "de": "Hallo?"]

        /// Why the telephone is disabled (`callAvailability`, in order).
        public enum Reasons {
            public static let noVoiceDevice = "Pick a voice device in Settings → Voice"
            public static let offline = "{device} is offline"
            public static let notHere = "Calls run on {device}"
            public static let preparing = "{device} is getting its voice ready"
            public static let noGpu = "{device} can't speak: no usable GPU"
            public static let busy = "{device} is in another call"
            /// #NEW-57: a meeting runs on the voice device.
            public static let meeting = "{device} is transcribing a meeting"
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
            "meeting": "{device} is transcribing a meeting",
            "unavailable": "{device} couldn't get its voice ready",
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
        public static let speaker = "voice-call-speaker"
        public static let panel = "voice-call-panel"
        public static let pill = "voice-call-pill"
        public static let timer = "voice-call-timer"
    }

    /// Icon concepts (`AppIcons.voiceCall`, `AppIcons.callMute`, …).
    public enum Icons {
        /// The telephone's icon concept.
        public static let call = "voice-call"
        public static let mute = "call-mute"
        public static let unmute = "call-unmute"
        public static let speaker = "call-speaker"
        public static let speakerOff = "call-speaker-off"
        public static let end = "call-end"
    }

    public enum Constants {
        /// S-3: how long the caller's ring plays before `rang`.
        public static let ringMs = 3_000
        /// A voice still not ready after this fails the call `unavailable`.
        public static let maxRingMs = 300_000
        /// The ring's mic check: a frame at or above this level = sound.
        public static let noSoundLevel = 0.1
        public static let ringTimeoutMs = 10_000
        public static let minSilenceMs = 700
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
        public var online: Bool
        /// THIS client is the voice device.
        public var here: Bool
        /// nil = no `speech` row yet.
        public var speech: Speech?
        /// A call already runs on the voice device.
        public var busy: Bool
        /// #NEW-57: a meeting runs on the voice device (`meetingStartedAt` set).
        public var meeting: Bool

        public init(
            platform: Platform, device: String?, online: Bool,
            here: Bool, speech: Speech?, busy: Bool, meeting: Bool = false
        ) {
            self.platform = platform
            self.device = device
            self.online = online
            self.here = here
            self.speech = speech
            self.busy = busy
            self.meeting = meeting
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
    /// (first match wins). S-3: a voice that is still downloading rings
    /// until it is ready.
    public static func callAvailability(_ input: AvailabilityInput) -> Availability {
        func off(_ reason: String) -> Availability {
            Availability(enabled: false, reason: reason.replacingOccurrences(of: "{device}", with: input.device ?? ""))
        }
        guard input.device != nil else { return off(Copy.Reasons.noVoiceDevice) }
        if !input.online { return off(Copy.Reasons.offline) }
        if input.meeting { return off(Copy.Reasons.meeting) }
        if input.platform == .desktop && !input.here { return off(Copy.Reasons.notHere) }
        if input.speech == nil { return off(Copy.Reasons.preparing) }
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

    public enum Phase: String, Sendable { case hidden, ringing, listening, thinking, speaking }

    public struct State: Equatable, Sendable {
        public var phase: Phase
        public var muted: Bool
        /// Phones: the loudspeaker (false = the earpiece).
        public var speaker: Bool
        public var device: String
        /// The greeting the device says once answered.
        public var greeting: String
        /// The ring's `ringMs` ran out.
        public var rang: Bool
        /// The worker confirmed it listens (`CallListening`).
        public var ready: Bool
        /// The microphone stayed silent during the ring.
        public var noSound: Bool
        /// The last utterance, quoted (what Thinking shows).
        public var said: String
        public var line: String
        public var toast: String?

        public init(
            phase: Phase, muted: Bool = false, speaker: Bool = true, device: String = "",
            greeting: String = "", rang: Bool = false, ready: Bool = false, noSound: Bool = false,
            said: String = "", line: String = "", toast: String? = nil
        ) {
            self.phase = phase
            self.muted = muted
            self.speaker = speaker
            self.device = device
            self.greeting = greeting
            self.rang = rang
            self.ready = ready
            self.noSound = noSound
            self.said = said
            self.line = line
            self.toast = toast
        }
    }

    /// One flow event (`flowCases`; the host sends rang, ready, noSound,
    /// partial, said, queued, replyStarted, replyFinished and speechStarted
    /// as `call_event`).
    public enum Event: Equatable, Sendable {
        case start(device: String, language: String? = nil)
        case rang
        case ready
        case noSound
        case partial(text: String)
        case said(text: String)
        case speechStarted
        case queued(count: Int, text: String)
        case replyStarted(text: String)
        case replyFinished
        case mute
        case unmute
        case speaker(on: Bool)
        case end
        case hungUp(reason: String?)
        case failed(code: String)

        /// The event a JSON object names by its `t`; nil for an unknown `t`.
        public init?(json: [String: Any]) {
            let text = json["text"] as? String ?? ""
            switch json["t"] as? String {
            case "start": self = .start(device: json["device"] as? String ?? "", language: json["language"] as? String)
            case "rang": self = .rang
            case "ready": self = .ready
            case "noSound": self = .noSound
            case "partial": self = .partial(text: text)
            case "said": self = .said(text: text)
            case "speechStarted": self = .speechStarted
            case "queued": self = .queued(count: (json["count"] as? NSNumber)?.intValue ?? 0, text: text)
            case "replyStarted": self = .replyStarted(text: text)
            case "replyFinished": self = .replyFinished
            case "mute": self = .mute
            case "unmute": self = .unmute
            case "speaker": self = .speaker(on: json["on"] as? Bool ?? true)
            case "end": self = .end
            case "hungUp": self = .hungUp(reason: json["reason"] as? String)
            case "failed": self = .failed(code: json["code"] as? String ?? "")
            default: return nil
            }
        }

        /// The events a HOST may send in a `call_event` frame; the caller
        /// owns start, mute, unmute, speaker, end and hungUp itself. A host
        /// that could not get its voice ready sends `failed {code}` before
        /// it ends the call (the caller fails with that code's toast).
        public var isHostEvent: Bool {
            switch self {
            case .rang, .ready, .noSound, .partial, .said, .queued, .replyStarted, .replyFinished, .speechStarted, .failed: true
            default: false
            }
        }
    }

    /// No call (`CALL_IDLE`).
    public static let idle = State(phase: .hidden)

    /// The greeting in `language` (English for an unknown one).
    public static func callGreeting(_ language: String?) -> String {
        Copy.greeting[language ?? "en"] ?? Copy.greeting["en"] ?? ""
    }

    private static func quoted(_ text: String) -> String { "“\(text)”" }

    private static func trimmed(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Rang and ready: the device speaks its greeting (not an agent turn).
    private static func answered(_ state: State) -> State {
        var next = state
        next.phase = .speaking
        next.line = state.greeting
        next.toast = nil
        return next
    }

    /// One step of the call (the same machine on the voice device and a caller).
    public static func callStep(_ state: State, _ event: Event) -> State {
        func at(_ phase: Phase, _ line: String, said: String? = nil, noSound: Bool? = nil) -> State {
            var next = state
            next.phase = phase
            next.line = line
            next.toast = nil
            if let said { next.said = said }
            if let noSound { next.noSound = noSound }
            return next
        }
        if case let .start(device, language) = event {
            var next = idle
            next.phase = .ringing
            next.device = device
            next.greeting = callGreeting(language)
            return next
        }
        if state.phase == .hidden { return state }
        let ringing = state.phase == .ringing
        switch event {
        case .start:
            return state
        case .rang:
            guard ringing else { return state }
            var next = state
            next.rang = true
            return state.ready ? answered(next) : next
        case .ready:
            guard ringing else { return state }
            var next = state
            next.ready = true
            return state.rang ? answered(next) : next
        case .noSound:
            var next = state
            next.noSound = true
            return next
        case let .partial(text):
            let text = trimmed(text)
            return state.phase == .listening && !text.isEmpty ? at(.listening, quoted(text), noSound: false) : state
        case let .said(text):
            let text = trimmed(text)
            guard !ringing, !text.isEmpty else { return state }
            return at(.thinking, quoted(text), said: quoted(text), noSound: false)
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
            return ringing ? state : at(.speaking, trimmed(text))
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
        case let .speaker(on):
            var next = state
            next.speaker = on
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

    /// The call surface's activity (the avatar ring's animation).
    public enum Activity: String, Sendable { case ringing, listening, thinking, speaking, muted }

    /// One round call button, in `View.buttons` order.
    public enum Button: String, Sendable { case mute, speaker, end }

    public struct View: Equatable, Sendable {
        public var activity: Activity
        /// The line under the name; nil = the call's timer (`callTimer`).
        public var status: String?
        public var caption: String
        public var muted: Bool
        public var speaker: Bool
        /// The round buttons, in order.
        public var buttons: [Button]
        /// The end button's label (shown under it while ringing).
        public var end: String

        public init(
            activity: Activity, status: String?, caption: String, muted: Bool,
            speaker: Bool, buttons: [Button], end: String
        ) {
            self.activity = activity
            self.status = status
            self.caption = caption
            self.muted = muted
            self.speaker = speaker
            self.buttons = buttons
            self.end = end
        }
    }

    /// What the call surface shows; nil = no surface.
    public static func callView(_ state: State, platform: Platform = .phone) -> View? {
        switch state.phase {
        case .hidden:
            return nil
        case .ringing:
            return View(
                activity: .ringing, status: state.rang ? Copy.gettingReady : Copy.calling, caption: "",
                muted: state.muted, speaker: state.speaker, buttons: [.end], end: Copy.cancel
            )
        case .listening, .thinking, .speaking:
            let buttons: [Button] = platform == .phone ? [.mute, .speaker, .end] : [.mute, .end]
            func inCall(_ activity: Activity, _ caption: String) -> View {
                View(
                    activity: activity, status: nil, caption: caption, muted: state.muted,
                    speaker: state.speaker, buttons: buttons, end: Copy.endCall
                )
            }
            switch state.phase {
            case .listening:
                if state.muted { return inCall(.muted, Copy.muted) }
                return inCall(.listening, state.line.isEmpty ? (state.noSound ? Copy.noSound : "") : state.line)
            case .thinking:
                return inCall(.thinking, state.line)
            default:
                return inCall(.speaking, state.line)
            }
        }
    }

    /// The call's timer since it was answered: mm:ss, h:mm:ss from an hour.
    public static func callTimer(ms: Int) -> String {
        let total = max(0, ms / 1000)
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let seconds = total % 60
        func pad(_ n: Int) -> String { n < 10 ? "0\(n)" : "\(n)" }
        return hours > 0 ? "\(hours):\(pad(minutes)):\(pad(seconds))" : "\(pad(minutes)):\(pad(seconds))"
    }

    // MARK: - Surface specimens

    /// One styleguide row of the call surface (`surfaceSpecimens`).
    public struct Specimen: Equatable, Sendable {
        public var name: String
        public var agent: String
        /// The timer's text in call; nil while ringing.
        public var timer: String?
        public var level: Double
        public var bands: [Double]
        public var view: View

        public init(name: String, agent: String, timer: String?, level: Double, bands: [Double], view: View) {
            self.name = name
            self.agent = agent
            self.timer = timer
            self.level = level
            self.bands = bands
            self.view = view
        }
    }

    private static let inCallButtons: [Button] = [.mute, .speaker, .end]

    public static let surfaceSpecimens: [Specimen] = [
        Specimen(name: "ringing", agent: "Assistant", timer: nil, level: 0, bands: [], view: View(
            activity: .ringing, status: "Calling…", caption: "", muted: false, speaker: true, buttons: [.end], end: "Cancel"
        )),
        Specimen(name: "getting ready", agent: "Assistant", timer: nil, level: 0, bands: [], view: View(
            activity: .ringing, status: "Getting ready…", caption: "", muted: false, speaker: true, buttons: [.end], end: "Cancel"
        )),
        Specimen(name: "in call, speaking", agent: "Assistant", timer: "04:12", level: 0.5, bands: [0.7, 1, 0.5, 0.8], view: View(
            activity: .speaking, status: nil,
            caption: "Batch 4 is rebasing; the other two are merged. Want me to merge the last one when it's green?",
            muted: false, speaker: true, buttons: inCallButtons, end: "End call"
        )),
        Specimen(name: "in call, listening", agent: "Assistant", timer: "04:20", level: 0.7, bands: [0.9, 0.6, 1, 0.5], view: View(
            activity: .listening, status: nil, caption: "“merge it when it's green”",
            muted: false, speaker: true, buttons: inCallButtons, end: "End call"
        )),
        Specimen(name: "muted", agent: "Assistant", timer: "04:31", level: 0, bands: [], view: View(
            activity: .muted, status: nil, caption: "Microphone muted",
            muted: true, speaker: true, buttons: inCallButtons, end: "End call"
        )),
    ]

    /// The floating pill's fixed size (desktop only; mirrored for parity).
    public enum Pill {
        public static let width = 320
        public static let height = 56
        public static let marginPx = 24
    }

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
