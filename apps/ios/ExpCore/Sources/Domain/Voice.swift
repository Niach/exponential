import Foundation

// NEW-67 (voice plan v3, V1): THE Settings → Voice section. A port of the TS
// reference `packages/domain-contract/src/voice.ts` (desktop `domain::voice`,
// Android `domain/Voice.kt`), byte-locked by `fixtures/voice.json` (`copy`,
// `testIds`, `shortcutGrammar`, `languages`, `limits`, `defaults` and the option builders, replayed by
// `VoiceTests`). The phones only show and edit the member's setting: speaking
// is desktop-only. The fixture's `$comment` holds the rules.

public enum Voice {
    public enum Copy {
        public static let group = "Voice"
        public static let voiceDevice = "Voice device"
        public static let voiceDeviceNone = "None"
        public static let agent = "Agent"
        public static let agentNone = "None"
        public static let language = "Language"
        public static let languageNote = "Needs a GPU on {device}"
        public static let voiceFile = "Voice file"
        public static let voiceFilePlaceholder = "Built-in"
        public static let voiceTranscript = "Transcript"
        public static let voiceTranscriptPlaceholder = "What the clip says"
        public static let trayHeader = "On {device} (tray)"
        public static let pushToTalk = "Push to talk"
        public static let wakeWord = "Wake word"
        public static let wakePhrase = "Wake phrase"
        public static let wakePhrasePlaceholder = "Hey {agent}"
        public static let trayOpen = "Open Exponential"
        public static let trayCall = "Call {agent}"
        public static let trayMute = "Mute microphone"
        public static let traySettings = "Voice settings…"
        public static let trayQuit = "Quit"
        public static let downloading = "Downloading voice…"
        public static let downloadingProgress = "Downloading voice… {received} of {total}"
        public static let downloadFailed = "Couldn't download the voice"
        public static let unavailable = "The voice stopped working"
        public static let speaking = "Speaking"
        public static let stop = "Stop"
        public static let link = "link"
        public static let ellipsis = "…"
        public static let wakePhraseTooShort = "The wake phrase needs at least 6 letters"
        /// S-3: the Push to talk row's value with no key bound.
        public static let shortcutNone = "None"
        public static let shortcutRecord = "Press any key or combination…"
        public static let shortcutClear = "Clear"
        public static let trayEndCall = "End call"
    }

    /// The push-to-talk words the phones mirror (`pttCopy`).
    public enum PttCopy {
        public static let shortcutNeedsModifier = "Hold Ctrl, Alt, Shift or Super with that key"
        /// S-3: the Wake word row's trailing detail by state (`wakeDetail`).
        public static let wakeDetail: [String: String] = [
            "listening": "Listening",
            "muted": "Microphone muted",
            "noAgent": "Pick an agent",
            "downloading": "Downloading speech recognition…",
            "micBusy": "A call is using the microphone",
            "meeting": "A meeting is using the microphone",
            "noMicrophone": "No microphone",
            "notGranted": "Microphone not allowed",
            "unavailable": "Speech recognition unavailable",
            "offline": "{device} is offline",
            "starting": "Starting…",
        ]
    }

    public enum TestIds {
        public static let page = "settings-voice"
        public static let voiceDevice = "settings-voice-device"
        public static let agent = "settings-voice-agent"
        public static let language = "settings-voice-language"
        public static let voiceFile = "settings-voice-file"
        public static let voiceTranscript = "settings-voice-transcript"
        public static let shortcut = "settings-voice-shortcut"
        public static let wakeWord = "settings-voice-wake-word"
        public static let wakePhrase = "settings-voice-wake-phrase"
        public static let shortcutClear = "settings-voice-shortcut-clear"
    }

    public static let maxChars = 600

    /// One language replies are spoken in (#NEW-73; `value` is what is stored).
    public struct Language: Equatable, Hashable, Sendable {
        public let value: String
        public let label: String

        public init(value: String, label: String) {
            self.value = value
            self.label = label
        }
    }

    public static let languages: [Language] = [
        Language(value: "en", label: "English"),
        Language(value: "de", label: "Deutsch"),
    ]

    /// The most characters the two voice-clip inputs store, and the fewest
    /// letters a non-empty wake phrase needs (`limits`).
    public enum Limits {
        public static let voiceFile = 512
        public static let voiceTranscript = 500
        /// The wake phrase's cap (`voice.update`'s `oneLine`).
        public static let wakePhrase = 64
        public static let wakePhraseMinLetters = 6
    }

    /// The member's ONE settings row (`voice.get`, `voice.update`'s result).
    public struct Settings: Codable, Equatable, Sendable {
        public var voiceDeviceId: String?
        public var agentId: String?
        /// S-3: the push-to-talk binding (`shortcutGrammar`); empty = no key
        /// bound = push to talk off.
        public var pttShortcut: String
        public var wakeWord: Bool
        public var wakePhrase: String
        /// #NEW-73: one of `languages`.
        public var language: String
        /// A reference WAV's path ON the voice device; empty = the built-in voice.
        public var voiceFile: String
        /// What that clip says.
        public var voiceTranscript: String
        /// #NEW-57 (meeting.json): the desktop audio a meeting transcribes.
        public var desktopAudio: Meeting.DesktopAudio
        /// The apps `include` picks or `exclude` leaves out.
        public var desktopAudioApps: [Meeting.AudioApp]
        /// ISO time the meeting started; nil = no meeting. Written only by
        /// `voice.startMeeting`, cleared by `voice.stopMeeting`.
        public var meetingStartedAt: String?

        /// A meeting runs on the voice device.
        public var meetingOn: Bool { meetingStartedAt != nil }

        public init(
            voiceDeviceId: String? = nil,
            agentId: String? = nil,
            pttShortcut: String = "",
            wakeWord: Bool = false,
            wakePhrase: String = "",
            language: String = "en",
            voiceFile: String = "",
            voiceTranscript: String = "",
            desktopAudio: Meeting.DesktopAudio = .off,
            desktopAudioApps: [Meeting.AudioApp] = [],
            meetingStartedAt: String? = nil
        ) {
            self.voiceDeviceId = voiceDeviceId
            self.agentId = agentId
            self.pttShortcut = pttShortcut
            self.wakeWord = wakeWord
            self.wakePhrase = wakePhrase
            self.language = language
            self.voiceFile = voiceFile
            self.voiceTranscript = voiceTranscript
            self.desktopAudio = desktopAudio
            self.desktopAudioApps = desktopAudioApps
            self.meetingStartedAt = meetingStartedAt
        }

        private enum CodingKeys: String, CodingKey {
            case voiceDeviceId, agentId, pttShortcut, wakeWord, wakePhrase
            case language, voiceFile, voiceTranscript, desktopAudio, desktopAudioApps, meetingStartedAt
        }

        /// A key the server does not send yet (an older server) reads its
        /// default; the legacy `speakReplies` / `pushToTalk` keys are ignored.
        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            let d = Settings()
            voiceDeviceId = try c.decodeIfPresent(String.self, forKey: .voiceDeviceId)
            agentId = try c.decodeIfPresent(String.self, forKey: .agentId)
            pttShortcut = try c.decodeIfPresent(String.self, forKey: .pttShortcut) ?? d.pttShortcut
            wakeWord = try c.decodeIfPresent(Bool.self, forKey: .wakeWord) ?? d.wakeWord
            wakePhrase = try c.decodeIfPresent(String.self, forKey: .wakePhrase) ?? d.wakePhrase
            language = try c.decodeIfPresent(String.self, forKey: .language) ?? d.language
            voiceFile = try c.decodeIfPresent(String.self, forKey: .voiceFile) ?? d.voiceFile
            voiceTranscript = try c.decodeIfPresent(String.self, forKey: .voiceTranscript) ?? d.voiceTranscript
            // An unknown value (a newer server) reads Off.
            desktopAudio = ((try? c.decodeIfPresent(String.self, forKey: .desktopAudio)) ?? nil)
                .flatMap(Meeting.DesktopAudio.init(rawValue:)) ?? d.desktopAudio
            desktopAudioApps = try c.decodeIfPresent([Meeting.AudioApp].self, forKey: .desktopAudioApps) ?? d.desktopAudioApps
            meetingStartedAt = try c.decodeIfPresent(String.self, forKey: .meetingStartedAt)
        }
    }

    /// The row a member without one reads (`defaults`).
    public static let defaults = Settings()

    /// A partial write (`voice.update`): nil = the key is absent (kept);
    /// `.some(nil)` for the device or the agent = None.
    public struct Change: Encodable, Equatable, Sendable {
        public var voiceDeviceId: String??
        public var agentId: String??
        public var pttShortcut: String?
        public var wakeWord: Bool?
        public var wakePhrase: String?
        public var language: String?
        public var voiceFile: String?
        public var voiceTranscript: String?
        /// #NEW-57: the Desktop audio row.
        public var desktopAudio: Meeting.DesktopAudio?
        /// #NEW-57: the Apps row.
        public var desktopAudioApps: [Meeting.AudioApp]?

        public init(
            voiceDeviceId: String?? = nil,
            agentId: String?? = nil,
            pttShortcut: String? = nil,
            wakeWord: Bool? = nil,
            wakePhrase: String? = nil,
            language: String? = nil,
            voiceFile: String? = nil,
            voiceTranscript: String? = nil,
            desktopAudio: Meeting.DesktopAudio? = nil,
            desktopAudioApps: [Meeting.AudioApp]? = nil
        ) {
            self.voiceDeviceId = voiceDeviceId
            self.agentId = agentId
            self.pttShortcut = pttShortcut
            self.wakeWord = wakeWord
            self.wakePhrase = wakePhrase
            self.language = language
            self.voiceFile = voiceFile
            self.voiceTranscript = voiceTranscript
            self.desktopAudio = desktopAudio
            self.desktopAudioApps = desktopAudioApps
        }

        private enum CodingKeys: String, CodingKey {
            case voiceDeviceId, agentId, pttShortcut, wakeWord, wakePhrase
            case language, voiceFile, voiceTranscript, desktopAudio, desktopAudioApps
        }

        /// None travels as an explicit `null`; an untouched field writes no key.
        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            if let voiceDeviceId {
                if let voiceDeviceId { try c.encode(voiceDeviceId, forKey: .voiceDeviceId) } else { try c.encodeNil(forKey: .voiceDeviceId) }
            }
            if let agentId {
                if let agentId { try c.encode(agentId, forKey: .agentId) } else { try c.encodeNil(forKey: .agentId) }
            }
            try c.encodeIfPresent(pttShortcut, forKey: .pttShortcut)
            try c.encodeIfPresent(wakeWord, forKey: .wakeWord)
            try c.encodeIfPresent(wakePhrase, forKey: .wakePhrase)
            try c.encodeIfPresent(language, forKey: .language)
            try c.encodeIfPresent(voiceFile, forKey: .voiceFile)
            try c.encodeIfPresent(voiceTranscript, forKey: .voiceTranscript)
            try c.encodeIfPresent(desktopAudio, forKey: .desktopAudio)
            try c.encodeIfPresent(desktopAudioApps, forKey: .desktopAudioApps)
        }
    }

    /// Apply a change; choosing None for the device or the agent clears the
    /// wake word, so picking again never turns the always-on microphone on;
    /// #NEW-57: and stops a running meeting. S-3: a bound key stays bound.
    public static func clearOnNone(_ before: Settings, _ change: Change) -> Settings {
        var after = before
        if let voiceDeviceId = change.voiceDeviceId { after.voiceDeviceId = voiceDeviceId }
        if let agentId = change.agentId { after.agentId = agentId }
        if let pttShortcut = change.pttShortcut { after.pttShortcut = pttShortcut }
        if let wakeWord = change.wakeWord { after.wakeWord = wakeWord }
        if let wakePhrase = change.wakePhrase { after.wakePhrase = wakePhrase }
        if let language = change.language { after.language = language }
        if let voiceFile = change.voiceFile { after.voiceFile = voiceFile }
        if let voiceTranscript = change.voiceTranscript { after.voiceTranscript = voiceTranscript }
        if let desktopAudio = change.desktopAudio { after.desktopAudio = desktopAudio }
        if let desktopAudioApps = change.desktopAudioApps { after.desktopAudioApps = desktopAudioApps }
        if after.voiceDeviceId == nil || after.agentId == nil {
            after.wakeWord = false
            // #NEW-57: None for the device or the agent stops the meeting.
            after.meetingStartedAt = nil
        }
        return after
    }

    /// One picker option; `id` nil = None.
    public struct Option: Equatable, Hashable, Sendable {
        public let id: String?
        public let label: String

        public init(id: String?, label: String) {
            self.id = id
            self.label = label
        }
    }

    /// One device the member owns (`kind` = contract `deviceKind`).
    public struct DeviceLike: Equatable, Sendable {
        public let deviceId: String
        public let name: String
        public let kind: String

        public init(deviceId: String, name: String, kind: String) {
            self.deviceId = deviceId
            self.name = name
            self.kind = kind
        }
    }

    /// One agent the member can pick (`team` = its team's name).
    public struct AgentLike: Equatable, Sendable {
        public let id: String
        public let name: String
        public let team: String

        public init(id: String, name: String, team: String) {
            self.id = id
            self.name = name
            self.team = team
        }
    }

    private static let english = Locale(identifier: "en")

    private static func byLabel(_ a: Option, _ b: Option) -> Bool {
        a.label.compare(b.label, locale: english) == .orderedAscending
    }

    /// The Voice device picker's options: None first, then the member's
    /// desktops by name (phones and servers are never a voice device).
    public static func voiceDeviceOptions(_ devices: [DeviceLike]) -> [Option] {
        let options = devices
            .filter { $0.kind == "desktop" }
            .map { Option(id: $0.deviceId, label: $0.name) }
            .sorted(by: byLabel)
        return [Option(id: nil, label: Copy.voiceDeviceNone)] + options
    }

    /// The Agent picker's options: None first, then the agents by label
    /// (`{name} · {team}` when two agents share a name).
    public static func agentOptions(_ agents: [AgentLike]) -> [Option] {
        var counts: [String: Int] = [:]
        for agent in agents { counts[agent.name, default: 0] += 1 }
        let options = agents
            .map { agent in
                Option(
                    id: agent.id,
                    label: (counts[agent.name] ?? 0) > 1 ? "\(agent.name) · \(agent.team)" : agent.name
                )
            }
            .sorted(by: byLabel)
        return [Option(id: nil, label: Copy.agentNone)] + options
    }

    /// The tray group's header for the voice device.
    public static func trayHeader(_ deviceName: String) -> String {
        Copy.trayHeader.replacingOccurrences(of: "{device}", with: deviceName)
    }

    /// The wake phrase's placeholder (and its value when empty).
    public static func wakePhrasePlaceholder(_ agentName: String) -> String {
        Copy.wakePhrasePlaceholder.replacingOccurrences(of: "{agent}", with: agentName)
    }

    /// #NEW-75: THE rule of the page's one-line inputs, as you type
    /// (`voiceTextInput`): each run of whitespace controls (tab, LF, VT, FF,
    /// CR, NEL) becomes one space (#FEED-160), every other control character
    /// (Cc: C0, DEL, C1) is dropped, then the value is cut to at most `max`
    /// UTF-16 code units (the server's zod count, not Characters), whole
    /// code points only. Trimming waits for the save.
    public static func textInput(_ value: String, max: Int) -> String {
        var clean: [Unicode.Scalar] = []
        var inRun = false
        for scalar in value.unicodeScalars {
            switch scalar.value {
            case 0x09 ... 0x0D, 0x85:
                if !inRun { clean.append(" ") }
                inRun = true
            default:
                inRun = false
                if scalar.properties.generalCategory != .control { clean.append(scalar) }
            }
        }
        var out = String.UnicodeScalarView()
        var used = 0
        for scalar in clean {
            used += UTF16.width(scalar)
            if used > max { break }
            out.append(scalar)
        }
        return String(out)
    }

    /// #NEW-56: a non-empty wake phrase needs `Limits.wakePhraseMinLetters`
    /// letters (spaces, digits and punctuation don't count); nil = valid. An
    /// empty phrase is valid (it means `wakePhrasePlaceholder`).
    public static func wakePhraseError(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let letters = trimmed.filter(\.isLetter).count
        return letters < Limits.wakePhraseMinLetters ? Copy.wakePhraseTooShort : nil
    }

    /// #NEW-73: the Language row's trailing detail; every language speaks on
    /// the voice device's GPU only (#NEW-74). Nil without a voice device.
    public static func languageNote(language _: String, deviceName: String?) -> String? {
        guard let deviceName else { return nil }
        return Copy.languageNote.replacingOccurrences(of: "{device}", with: deviceName)
    }

    /// #NEW-73: what the Language row and its menu show for a language.
    public static func languageLabel(language: String, deviceName: String?) -> String {
        let label = languages.first { $0.value == language }?.label ?? language
        guard let note = languageNote(language: language, deviceName: deviceName) else { return label }
        return "\(label) · \(note)"
    }

    // MARK: - S-3: push-to-talk key bindings

    /// `shortcutGrammar`: what a stored binding may be.
    public enum ShortcutGrammar {
        public static let mods = ["ctrl", "alt", "shift", "super"]
        public static let named = [
            "space", "enter", "tab", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown",
            "up", "down", "left", "right", "minus", "equal", "bracketleft", "bracketright", "backslash",
            "semicolon", "quote", "comma", "period", "slash", "backquote", "pause", "scrolllock", "printscreen",
        ]
        public static let bareKeys = ["f1-f24", "insert", "pause", "scrolllock", "printscreen", "mouse3-mouse9"]
        public static let mouse = 3...9
        public static let function = 1...24
        public static let maxLength = 32
        public static let labels: [String: String] = [
            "ctrl": "Ctrl", "alt": "Alt", "shift": "Shift", "super": "Super", "space": "Space",
            "enter": "Enter", "tab": "Tab", "backspace": "Backspace", "delete": "Delete", "insert": "Insert",
            "home": "Home", "end": "End", "pageup": "Page Up", "pagedown": "Page Down", "up": "Up",
            "down": "Down", "left": "Left", "right": "Right", "minus": "-", "equal": "=",
            "bracketleft": "[", "bracketright": "]", "backslash": "\\", "semicolon": ";", "quote": "'",
            "comma": ",", "period": ".", "slash": "/", "backquote": "`", "pause": "Pause",
            "scrolllock": "Scroll Lock", "printscreen": "Print Screen", "mouse": "Mouse button {n}",
        ]
        public static let separator = " + "
    }

    /// `prefix` followed by one or two decimal digits (no leading zero for
    /// two), as the number; nil otherwise.
    private static func numbered(_ part: String, _ prefix: String, maxDigits: Int) -> Int? {
        guard part.hasPrefix(prefix) else { return nil }
        let digits = part.dropFirst(prefix.count)
        guard !digits.isEmpty, digits.count <= maxDigits, digits.allSatisfy({ $0.isASCII && $0.isNumber }) else { return nil }
        if digits.count > 1 && digits.first == "0" { return nil }
        return Int(digits)
    }

    /// The grammar's key (not a modifier): a letter, digit, f1-f24, a named
    /// key or mouse3-mouse9.
    private static func isShortcutKey(_ part: String) -> Bool {
        if part.count == 1, let c = part.unicodeScalars.first, ("a"..."z").contains(c) || ("0"..."9").contains(c) {
            return true
        }
        if let n = numbered(part, "f", maxDigits: 2) {
            return ShortcutGrammar.function.contains(n)
        }
        if let n = numbered(part, "mouse", maxDigits: 1) { return ShortcutGrammar.mouse.contains(n) }
        return ShortcutGrammar.named.contains(part)
    }

    /// A key that may be bound without a modifier (`bareKeys`).
    private static func isBareKey(_ part: String) -> Bool {
        numbered(part, "f", maxDigits: 2) != nil || numbered(part, "mouse", maxDigits: 1) != nil
            || ["insert", "pause", "scrolllock", "printscreen"].contains(part)
    }

    /// S-3: the canonical text of a binding (`shortcutGrammar`), or nil when
    /// the grammar refuses it; empty stays empty (no key = off).
    public static func shortcutCanonical(_ text: String) -> String? {
        if text.isEmpty { return "" }
        var parts = text.components(separatedBy: "+")
        let key = parts.removeLast()
        guard isShortcutKey(key) else { return nil }
        guard parts.allSatisfy(ShortcutGrammar.mods.contains), Set(parts).count == parts.count else { return nil }
        if parts.isEmpty && !isBareKey(key) { return nil }
        let value = (ShortcutGrammar.mods.filter(parts.contains) + [key]).joined(separator: "+")
        return value.count > ShortcutGrammar.maxLength ? nil : value
    }

    private static func partLabel(_ part: String) -> String {
        if let n = numbered(part, "mouse", maxDigits: 1) {
            return (ShortcutGrammar.labels["mouse"] ?? "").replacingOccurrences(of: "{n}", with: String(n))
        }
        return ShortcutGrammar.labels[part] ?? part.uppercased()
    }

    /// S-3: a binding's label (`Ctrl + Shift + K`); empty reads None.
    public static func shortcutLabel(_ value: String) -> String {
        if value.isEmpty { return Copy.shortcutNone }
        return value.components(separatedBy: "+").map(partLabel).joined(separator: ShortcutGrammar.separator)
    }

    /// The push-to-talk recorder (the voice device's IDE records; the
    /// phones replay it for parity).
    public struct CaptureState: Equatable, Sendable {
        public enum Status: String, Sendable { case recording, done, cancelled }

        public var status: Status
        /// The binding (the one before recording until a key records).
        public var value: String
        /// The modifiers held now, in canonical order.
        public var held: [String]
        public var error: String?

        public init(status: Status, value: String, held: [String] = [], error: String? = nil) {
            self.status = status
            self.value = value
            self.held = held
            self.error = error
        }
    }

    public enum CaptureEvent: Equatable, Sendable {
        case down(key: String)
        case up(key: String)
        case mouse(button: Int)
    }

    public static func captureStart(_ value: String) -> CaptureState {
        CaptureState(status: .recording, value: value)
    }

    /// S-3: the push-to-talk recorder (`captureCases`).
    public static func captureStep(_ state: CaptureState, _ event: CaptureEvent) -> CaptureState {
        guard state.status == .recording else { return state }
        func record(_ key: String) -> CaptureState {
            guard let value = shortcutCanonical((state.held + [key]).joined(separator: "+")) else {
                var next = state
                next.error = PttCopy.shortcutNeedsModifier
                return next
            }
            return CaptureState(status: .done, value: value)
        }
        let key: String
        let down: Bool
        switch event {
        case let .mouse(button):
            return button < ShortcutGrammar.mouse.lowerBound ? state : record("mouse\(button)")
        case let .down(k): key = k; down = true
        case let .up(k): key = k; down = false
        }
        var next = state
        if ShortcutGrammar.mods.contains(key) {
            let held = down ? state.held + [key] : state.held.filter { $0 != key }
            next.held = ShortcutGrammar.mods.filter(held.contains)
            return next
        }
        if !down { return state }
        if state.held.isEmpty && key == "escape" {
            next.status = .cancelled
            next.held = []
            return next
        }
        if state.held.isEmpty && (key == "backspace" || key == "delete") {
            return CaptureState(status: .done, value: "")
        }
        if key == "escape" {
            next.error = PttCopy.shortcutNeedsModifier
            return next
        }
        guard isShortcutKey(key) else { return state }
        return record(key)
    }

    /// S-3: the held modifiers while recording (`Ctrl + …`; empty = none).
    public static func capturePending(_ state: CaptureState) -> String {
        guard state.status == .recording, !state.held.isEmpty else { return "" }
        return state.held.map(partLabel).joined(separator: ShortcutGrammar.separator) + ShortcutGrammar.separator + "…"
    }

    // MARK: - S-3: wake detail

    /// Why the wake word is or is not listening (`pttCopy.wakeDetail` keys
    /// but noAgent / offline / starting).
    public enum WakeState: String, Sendable {
        case listening, muted, noAgent, downloading, micBusy, meeting, noMicrophone, notGranted, unavailable, offline, starting
    }

    public struct WakeDetailContext: Equatable, Sendable {
        public var wakeWord: Bool
        /// An agent is picked.
        public var agent: Bool
        /// The voice device is online.
        public var online: Bool
        /// The voice device's name.
        public var device: String
        /// What the worker reported; nil = nothing yet.
        public var state: WakeState?

        public init(wakeWord: Bool, agent: Bool, online: Bool, device: String, state: WakeState?) {
            self.wakeWord = wakeWord
            self.agent = agent
            self.online = online
            self.device = device
            self.state = state
        }
    }

    /// S-3: the Wake word row's trailing detail ×4 (nil = none). The phones
    /// show the voice device's doctor row `wake` detail verbatim when it
    /// reported one (`wakeDoctorDetail`), else this.
    public static func wakeDetail(_ context: WakeDetailContext) -> String? {
        let words = PttCopy.wakeDetail
        guard context.wakeWord else { return nil }
        guard context.agent else { return words["noAgent"] }
        guard context.online else { return words["offline"]?.replacingOccurrences(of: "{device}", with: context.device) }
        guard let state = context.state else { return words["starting"] }
        return words[state.rawValue]?.replacingOccurrences(of: "{device}", with: context.device)
    }

    /// The voice device's doctor row `wake` (group voice) `detail`; nil
    /// when absent or empty.
    public static func wakeDoctorDetail(ofJSON json: String?) -> String? {
        let row = DeviceDoctor.decode(json: json)?.items.first { $0.key == "wake" && $0.group == "voice" }
        guard let detail = row?.detail, !detail.isEmpty else { return nil }
        return detail
    }
}
