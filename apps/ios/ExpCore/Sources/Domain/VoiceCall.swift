import Foundation

// NEW-69 (voice V5): CALLER MODE on the phones. A port of the TS reference
// `packages/etalk/src/voice-call.ts` (desktop `etalk::call`,
// Android `domain/VoiceCall.kt`), byte-locked by `fixtures/voice-call.json`
// (`copy`, `testIds`, `icons`, `constants`, `wire`, `surfaceSpecimens`, every
// `availabilityCases`, `targetCases` and `flowCases` row, replayed by
// `VoiceCallTests`). EXP-1857: `callTarget` picks the desktop that speaks
// (the agent's own, else the fallback the `via` line names). The
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
        /// EXP-1681: the phone surface's minimize button.
        public static let minimize = "Minimize"
        public static let queued = "{count} queued · {text}"
        /// EXP-1857: the second status line while the call falls back.
        public static let via = "Voice on {device}"

        /// Why the telephone is disabled (`callTarget`, `callAvailability`).
        public enum Reasons {
            public static let offline = "{device} is offline"
            public static let preparing = "{device} is getting its voice ready"
            /// NEW-99: the voice device's voice failed to load.
            public static let voiceFailed = "{device} couldn't load its voice: restart Exponential there"
            public static let noGpu = "{device} can't speak: no usable GPU"
            public static let busy = "{device} is in another call"
            /// #NEW-57: a meeting runs on the voice device.
            public static let meeting = "{device} is transcribing a meeting"
            /// EXP-1857: the member has no desktop at all.
            public static let noDesktop = "Open Exponential on one of your desktops to call"
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
        public static let minimize = "voice-call-minimize"
        /// EXP-1857: the fallback's second status line.
        public static let via = "voice-call-via"
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
        public static let minimize = "ui-chevron-down"
    }

    public enum Constants {
        /// EXP-1423: a ring's earliest answer is at least this long (`callRingMs`).
        public static let minRingMs = 3_000
        /// EXP-1423: up to this much more, drawn once per call by the host.
        public static let ringJitterMs = 6_000
        /// EXP-1423: the ring tone's cadence (`callRingToneOn`): on, then off.
        public static let ringToneMs = 2_000
        public static let ringGapMs = 4_000
        /// EXP-1423: the ring tone's two frequencies, sounding together.
        public static let ringLowHz = 440
        public static let ringHighHz = 480
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
        /// EXP-1929: the relay's beat (`call_alive`) once paired.
        public static let aliveMs = 5000
        /// EXP-1858: a server's speech worker exits after this long without a call.
        public static let serverIdleMs = 600_000
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
    public enum Speech: String, Sendable { case ok, preparing, noGpu, failed }

    /// EXP-1991: ONE desktop, as `callTarget` reads it for its reason.
    public struct AvailabilityInput: Equatable, Sendable {
        /// The desktop's name.
        public var device: String
        public var online: Bool
        /// nil = no `speech` row yet.
        public var speech: Speech?
        /// #NEW-57: a meeting runs on it (`meetingStartedAt` set).
        public var meeting: Bool

        public init(device: String, online: Bool, speech: Speech?, meeting: Bool = false) {
            self.device = device
            self.online = online
            self.speech = speech
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

    /// #EXP-1878 (`tapCases`): what a press on the telephone does: call,
    /// or (a phone's tap on a disabled one) toast the reason; web and the
    /// desktop show it as the disabled control's tooltip.
    public struct Tap: Equatable, Sendable {
        public var call: Bool
        public var toast: String?

        public init(call: Bool, toast: String?) {
            self.call = call
            self.toast = toast
        }
    }

    public static func callTap(_ platform: Platform, _ availability: Availability) -> Tap {
        if availability.enabled { return Tap(call: true, toast: nil) }
        return Tap(call: false, toast: platform == .phone ? availability.reason : nil)
    }

    /// Whether a desktop can take a call, and the reason when it cannot
    /// (`availabilityCases`, first match wins). S-3: a voice that is still
    /// downloading rings until it is ready.
    public static func callAvailability(_ input: AvailabilityInput) -> Availability {
        func off(_ reason: String) -> Availability {
            Availability(enabled: false, reason: reason.replacingOccurrences(of: "{device}", with: input.device))
        }
        if !input.online { return off(Copy.Reasons.offline) }
        if input.meeting { return off(Copy.Reasons.meeting) }
        if input.speech == nil { return off(Copy.Reasons.preparing) }
        if input.speech == .failed { return off(Copy.Reasons.voiceFailed) }
        if input.speech == .noGpu { return off(Copy.Reasons.noGpu) }
        return Availability(enabled: true, reason: nil)
    }

    // MARK: - Target

    /// EXP-1857: one of the member's own desktops as `callTarget` reads it.
    public struct TargetDevice: Equatable, Sendable {
        public var id: String
        public var name: String
        public var online: Bool
        /// Its doctor row `speech` through `callSpeech` (nil = none yet).
        public var speech: Speech?
        /// A meeting runs on it.
        public var meeting: Bool

        public init(id: String, name: String, online: Bool, speech: Speech?, meeting: Bool = false) {
            self.id = id
            self.name = name
            self.online = online
            self.speech = speech
            self.meeting = meeting
        }

        /// It can run a call now (a downloading voice rings until ready).
        public var speaks: Bool {
            online && !meeting && (speech == .ok || speech == .preparing)
        }
    }

    public struct TargetInput: Equatable, Sendable {
        /// THIS client's own desktop (nil on phones and web).
        public var here: String?
        /// The called agent's Runs on device.
        public var agentDevice: String?
        /// The member's own desktops, newest `lastSeenAt` first.
        public var desktops: [TargetDevice]
        /// EXP-1858: the member's own servers (CLI daemons): a candidate
        /// only as the agent's device, never here nor a fallback.
        public var servers: [TargetDevice]
        /// A call this client does not run already runs on the target.
        public var busy: Bool

        public init(here: String?, agentDevice: String?, desktops: [TargetDevice], servers: [TargetDevice] = [], busy: Bool) {
            self.here = here
            self.agentDevice = agentDevice
            self.desktops = desktops
            self.servers = servers
            self.busy = busy
        }
    }

    public struct Target: Equatable, Sendable {
        public var enabled: Bool
        public var reason: String?
        /// The desktop whose speech runs the call (nil = disabled).
        public var device: String?
        public var name: String?
        /// The target is not the agent's own device.
        public var fallback: Bool
        /// The target is this client's own desktop: it runs the call itself.
        public var local: Bool

        public init(enabled: Bool, reason: String?, device: String?, name: String?, fallback: Bool, local: Bool) {
            self.enabled = enabled
            self.reason = reason
            self.device = device
            self.name = name
            self.fallback = fallback
            self.local = local
        }

        public var availability: Availability { Availability(enabled: enabled, reason: reason) }
    }

    /// EXP-1857 (`targetCases`): where a call's speech runs. The first
    /// desktop that speaks of: the agent's device (one of the member's own
    /// desktops, EXP-1858: or servers), the desktop you're at, the other
    /// desktops newest first. None speaks = disabled with the first
    /// candidate's reason.
    public static func callTarget(_ input: TargetInput) -> Target {
        func byId(_ id: String?) -> TargetDevice? {
            guard let id else { return nil }
            return input.desktops.first { $0.id == id }
        }
        let agentDevice = byId(input.agentDevice) ?? input.agentDevice.flatMap { id in input.servers.first { $0.id == id } }
        let candidates = [agentDevice, byId(input.here)].compactMap { $0 } + input.desktops
        func off(_ reason: String?) -> Target {
            Target(enabled: false, reason: reason, device: nil, name: nil, fallback: false, local: false)
        }
        guard let target = candidates.first(where: \.speaks) else {
            guard let first = candidates.first else { return off(Copy.Reasons.noDesktop) }
            return off(callAvailability(AvailabilityInput(
                device: first.name, online: first.online, speech: first.speech, meeting: first.meeting
            )).reason)
        }
        if input.busy { return off(Copy.Reasons.busy.replacingOccurrences(of: "{device}", with: target.name)) }
        return Target(
            enabled: true, reason: nil, device: target.id, name: target.name,
            fallback: target.id != input.agentDevice, local: target.id == input.here
        )
    }

    /// NEW-99 (`speechCases`): the doctor row `speech` state as a caller
    /// reads it: nil → nil, ok → ok, missing (no usable GPU) → noGpu,
    /// error → failed, anything else → preparing (TS `callSpeechOf`).
    public static func callSpeech(state: String?) -> Speech? {
        switch state {
        case nil: return nil
        case "ok": return .ok
        case "missing": return .noGpu
        case "error": return .failed
        default: return .preparing
        }
    }

    /// The doctor's `speech` row (group `voice`), absent → nil.
    public static func callSpeech(of doctor: DeviceDoctor?) -> Speech? {
        guard let row = doctor?.items.first(where: { $0.key == "speech" && $0.group == "voice" }) else { return nil }
        return callSpeech(state: row.state)
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
        /// EXP-1857: the device is not the agent's own (`Copy.via` shows it).
        public var fallback: Bool
        /// The ring's earliest answer (`callRingMs`) passed.
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
            fallback: Bool = false, rang: Bool = false, ready: Bool = false, noSound: Bool = false,
            said: String = "", line: String = "", toast: String? = nil
        ) {
            self.phase = phase
            self.muted = muted
            self.speaker = speaker
            self.device = device
            self.fallback = fallback
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
        case start(device: String, language: String? = nil, fallback: Bool = false)
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
            case "start":
                self = .start(
                    device: json["device"] as? String ?? "", language: json["language"] as? String,
                    fallback: json["fallback"] as? Bool ?? false
                )
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

    private static func quoted(_ text: String) -> String { "“\(text)”" }

    private static func trimmed(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Rang and ready: it listens (#EXP-1966: no fixed greeting; the agent's
    /// own opening line, if any, arrives as its first reply).
    private static func answered(_ state: State) -> State {
        var next = state
        next.phase = .listening
        next.line = ""
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
        if case let .start(device, _, fallback) = event {
            var next = idle
            next.phase = .ringing
            next.device = device
            next.fallback = fallback
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
        /// EXP-1857: the device that speaks while the call falls back (a
        /// second status line); nil otherwise.
        public var via: String?
        public var caption: String
        public var muted: Bool
        public var speaker: Bool
        /// The round buttons, in order.
        public var buttons: [Button]
        /// The end button's label (shown under it while ringing).
        public var end: String

        public init(
            activity: Activity, status: String?, via: String? = nil, caption: String, muted: Bool,
            speaker: Bool, buttons: [Button], end: String
        ) {
            self.activity = activity
            self.status = status
            self.via = via
            self.caption = caption
            self.muted = muted
            self.speaker = speaker
            self.buttons = buttons
            self.end = end
        }
    }

    /// What the call surface shows; nil = no surface.
    public static func callView(_ state: State, platform: Platform = .phone) -> View? {
        let via = state.fallback ? Copy.via.replacingOccurrences(of: "{device}", with: state.device) : nil
        switch state.phase {
        case .hidden:
            return nil
        case .ringing:
            return View(
                activity: .ringing, status: state.rang ? Copy.gettingReady : Copy.calling, via: via, caption: "",
                muted: state.muted, speaker: state.speaker, buttons: [.end], end: Copy.cancel
            )
        case .listening, .thinking, .speaking:
            let buttons: [Button] = platform == .phone ? [.mute, .speaker, .end] : [.mute, .end]
            func inCall(_ activity: Activity, _ caption: String) -> View {
                View(
                    activity: activity, status: nil, via: via, caption: caption, muted: state.muted,
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

    /// EXP-1423: the earliest answer of a ring, ms after it started:
    /// `minRingMs` + floor(clamp(random, 0, 1) × `ringJitterMs`). The HOST
    /// draws `random` once per call; a caller never draws.
    public static func callRingMs(random: Double) -> Int {
        let unit = random.isNaN ? 0 : min(1, max(0, random))
        return Constants.minRingMs + Int((unit * Double(Constants.ringJitterMs)).rounded(.down))
    }

    /// EXP-1423: when the ring ends (answered), ms after it started: the
    /// later of the earliest answer and the warm-up's `ready`; nil = never
    /// answered (no ready, or one at or past `maxRingMs`: failed `unavailable`).
    public static func callAnswerMs(random: Double, readyMs: Int?) -> Int? {
        guard let readyMs, readyMs < Constants.maxRingMs else { return nil }
        return max(callRingMs(random: random), readyMs)
    }

    /// EXP-1423: whether the ring tone sounds `ms` after the ring started
    /// (the shared cadence: `ringToneMs` on, `ringGapMs` off, looping).
    public static func callRingToneOn(ms: Int) -> Bool {
        ms >= 0 && ms % (Constants.ringToneMs + Constants.ringGapMs) < Constants.ringToneMs
    }

    /// EXP-1929 (`livenessCases`): an end of a call that got a `call_alive`
    /// (`alive`) and has heard nothing from the relay for `silentMs` treats
    /// the path as dead (the call ends as dropped); before the first beat (an
    /// older relay) there is no deadline.
    public static func callPathDead(alive: Bool, silentMs: Int) -> Bool {
        alive && silentMs >= Constants.idleTimeoutMs
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
        Specimen(name: "in call, falling back", agent: "Assistant", timer: "04:20", level: 0.7, bands: [0.9, 0.6, 1, 0.5], view: View(
            activity: .listening, status: nil, via: "Voice on mint", caption: "“merge it when it's green”",
            muted: false, speaker: true, buttons: inCallButtons, end: "End call"
        )),
    ]

    /// FEED-272 (`tabs`): the Chat / Voice tabs on top of the call surface.
    public enum Tab: String, CaseIterable, Hashable, Sendable {
        case chat
        case voice
    }

    public enum Tabs {
        public static let order: [Tab] = [.chat, .voice]
        /// A call opens on Voice.
        public static let initial: Tab = .voice
        public static let tabsId = "voice-call-tabs"

        public static func label(_ tab: Tab) -> String {
            switch tab {
            case .chat: "Chat"
            case .voice: "Voice"
            }
        }

        public static func testId(_ tab: Tab) -> String { "voice-call-tab-\(tab.rawValue)" }
    }

    /// The floating pill's fixed size (the desktop's; EXP-1681 the phones'
    /// minimized call, never wider than the screen minus two margins).
    public enum Pill {
        public static let width = 320
        public static let height = 56
        public static let marginPx = 24
        /// The phone pill's default bottom edge above the safe area's.
        public static let phoneBottomPx = 96
    }

    // MARK: - Transport

    /// `voice.startCall`'s answer: the caller's relay ticket and the URL to
    /// dial AS-IS.
    /// EXP-1857: the server picks the target itself and answers it
    /// (`deviceId`, `device` = its name, `fallback`); optional so an older
    /// server still decodes. #EXP-1941: `seal` = the mode the server decided
    /// for both ends (true only for a caller that claimed it).
    public struct Ticket: Decodable, Equatable, Sendable {
        public let callId: String
        public let ticket: String
        public let url: String
        public let deviceId: String?
        public let device: String?
        public let fallback: Bool?
        public let seal: Bool?

        public init(
            callId: String, ticket: String, url: String,
            deviceId: String? = nil, device: String? = nil, fallback: Bool? = nil, seal: Bool? = nil
        ) {
            self.callId = callId
            self.ticket = ticket
            self.url = url
            self.deviceId = deviceId
            self.device = device
            self.fallback = fallback
            self.seal = seal
        }
    }

    // MARK: - #EXP-1941: the call seal's handshake steps (`seal`, `seal.modeCases`)

    /// What a caller reads of `seal` (the sealer is the eNet binding's
    /// `EnetCallHandshake`, the line is `CallSealLine`).
    public enum Seal {
        public static let cap = "call-seal"
        public static let version: UInt8 = 1
        public static let roleCaller: UInt8 = 1
        public static let roleHost: UInt8 = 2
        public static let typeHello: UInt8 = 1
        public static let callerHelloBytes = 54
        public static let hostHelloBytes = 70
        public static let helloEveryMs = 500
        public static let handshakeMs = 20_000
        public static let pullEveryMs = 2_000
        public static let maxOpenFailures = 50
        public static let ownDeviceCodes = ["no_microphone", "not_granted", "unsupported"]
        public static let noKey = "no_key"
        public static let sealReason = "seal"
        public static let required = false

        /// A wire code about the caller's OWN device is never taken from the
        /// wire (`error`, `bye`, a host's `failed`): it reads `dropped`.
        public static func wireCode(_ code: String) -> String {
            ownDeviceCodes.contains(code) ? HangUpRules.failedCode : code
        }
    }

    public enum SealPhase: String, Sendable { case idle, clear, told, offered, keying, sealed, over }

    public struct SealState: Equatable, Sendable {
        public var phase: SealPhase
        /// Sealed audio frames in a row that did not open.
        public var failures = 0

        public static let idle = SealState(phase: .idle)
    }

    public enum SealEvent: Equatable, Sendable {
        /// What `startCall` answered.
        case start(seal: Bool, required: Bool?)
        case ready
        /// The host's hello; `ok` = well-formed (length, version, role), checked before `finish`.
        case hello(ok: Bool)
        /// Root or epoch still differ, the rotation is still due, an epoch below the highest held.
        case noKey
        case key
        case finished(ok: Bool)
        case frame(opens: Bool)
        case sealed(opens: Bool)
        /// A `call_event` that is neither `hello`, `sealed` nor `failed`.
        case clear
        case failed(code: String)
        case closed
        /// `handshake` false = `maxCall` (the host's timer).
        case timeout(handshake: Bool)
    }

    /// The CALLER's step of TS `callSealStep`; the actions are the fixture's
    /// strings (`timer`, `hello`, `helloStop`, `key`, `finish`, `go`, `count`,
    /// `end:<reason>`, `fail:<code>`, `abandon`).
    public static func sealStep(_ state: SealState, _ event: SealEvent) -> (state: SealState, actions: [String]) {
        let dropped = HangUpRules.failedCode
        let stay = (state, [String]())
        func to(_ phase: SealPhase, _ actions: String...) -> (state: SealState, actions: [String]) { (SealState(phase: phase), actions) }
        func over(_ reason: String) -> (state: SealState, actions: [String]) { to(.over, "end:\(reason)", "fail:\(dropped)", "abandon") }
        if case let .start(seal, required) = event {
            guard state.phase == .idle else { return stay }
            if seal { return to(.told) }
            return required ?? Seal.required ? to(.over, "fail:\(dropped)") : to(.clear)
        }
        if case .timeout(handshake: false) = event { return stay }
        if [.idle, .clear, .over].contains(state.phase) { return stay }
        let sealed = state.phase == .sealed
        let keying = state.phase == .keying
        switch event {
        case .start: return stay
        case let .failed(code): return to(.over, "fail:\(Seal.wireCode(code))", "abandon")
        case .closed: return to(.over, "abandon")
        case .timeout: return sealed ? stay : over(Seal.noKey)
        case .ready: return state.phase == .told ? to(.offered, "timer", "hello") : stay
        case let .hello(ok):
            // The first well-formed hello is taken on arrival, once.
            let waiting = state.phase == .offered
            if waiting && ok { return to(.keying, "helloStop", "key") }
            return !ok && waiting ? (state, ["count"]) : stay
        case .noKey: return keying ? over(Seal.noKey) : stay
        case .key: return keying ? (state, ["finish"]) : stay
        case let .finished(ok):
            guard keying else { return stay }
            return ok ? to(.sealed, "go") : over(Seal.sealReason)
        case let .frame(opens):
            guard sealed else { return state.phase == .told ? stay : over(Seal.sealReason) }
            if opens { return state.failures == 0 ? stay : to(.sealed) }
            let failures = state.failures + 1
            if failures >= Seal.maxOpenFailures { return over(Seal.sealReason) }
            return (SealState(phase: .sealed, failures: failures), ["count"])
        case let .sealed(opens):
            if sealed { return opens ? stay : over(Seal.sealReason) }
            return state.phase == .told ? stay : over(Seal.sealReason)
        case .clear:
            return state.phase == .told ? stay : over(Seal.sealReason)
        }
    }
}
