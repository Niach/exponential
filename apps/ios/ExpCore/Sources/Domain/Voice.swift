import Foundation

// NEW-67 (voice plan v3, V1): THE Settings → Voice section. A port of the TS
// reference `packages/domain-contract/src/voice.ts` (desktop `domain::voice`,
// Android `domain/Voice.kt`), byte-locked by `fixtures/voice.json` (`copy`,
// `testIds`, `shortcuts`, `defaults` and the option builders, replayed by
// `VoiceTests`). The phones only show and edit the member's setting: speaking
// is desktop-only. The fixture's `$comment` holds the rules.

public enum Voice {
    public enum Copy {
        public static let group = "Voice"
        public static let voiceDevice = "Voice device"
        public static let voiceDeviceNone = "None"
        public static let agent = "Agent"
        public static let agentNone = "None"
        public static let speakReplies = "Speak replies"
        public static let trayHeader = "On {device} (tray)"
        public static let pushToTalk = "Push to talk"
        public static let shortcut = "Shortcut"
        public static let wakeWord = "Wake word"
        public static let wakePhrase = "Wake phrase"
        public static let wakePhrasePlaceholder = "Hey {agent}"
        public static let downloading = "Downloading voice…"
        public static let downloadFailed = "Couldn't download the voice"
        public static let unavailable = "The voice stopped working"
        public static let speaking = "Speaking"
        public static let stop = "Stop"
        public static let link = "link"
        public static let ellipsis = "…"
    }

    public enum TestIds {
        public static let page = "settings-voice"
        public static let voiceDevice = "settings-voice-device"
        public static let agent = "settings-voice-agent"
        public static let speakReplies = "settings-voice-speak-replies"
        public static let pushToTalk = "settings-voice-push-to-talk"
        public static let shortcut = "settings-voice-shortcut"
        public static let wakeWord = "settings-voice-wake-word"
        public static let wakePhrase = "settings-voice-wake-phrase"
    }

    public static let maxChars = 600

    /// One push-to-talk shortcut (`value` is what is stored).
    public struct Shortcut: Equatable, Hashable, Sendable {
        public let value: String
        public let label: String

        public init(value: String, label: String) {
            self.value = value
            self.label = label
        }
    }

    public static let shortcuts: [Shortcut] = [
        Shortcut(value: "ctrl+space", label: "Ctrl + Space"),
        Shortcut(value: "alt+space", label: "Alt + Space"),
        Shortcut(value: "ctrl+shift+space", label: "Ctrl + Shift + Space"),
        Shortcut(value: "f13", label: "F13"),
        Shortcut(value: "mouse4", label: "Mouse button 4"),
        Shortcut(value: "mouse5", label: "Mouse button 5"),
    ]

    /// The member's ONE settings row (`voice.get`, `voice.update`'s result).
    public struct Settings: Codable, Equatable, Sendable {
        public var voiceDeviceId: String?
        public var agentId: String?
        public var speakReplies: Bool
        public var pushToTalk: Bool
        public var pttShortcut: String
        public var wakeWord: Bool
        public var wakePhrase: String

        public init(
            voiceDeviceId: String? = nil,
            agentId: String? = nil,
            speakReplies: Bool = false,
            pushToTalk: Bool = false,
            pttShortcut: String = "ctrl+space",
            wakeWord: Bool = false,
            wakePhrase: String = ""
        ) {
            self.voiceDeviceId = voiceDeviceId
            self.agentId = agentId
            self.speakReplies = speakReplies
            self.pushToTalk = pushToTalk
            self.pttShortcut = pttShortcut
            self.wakeWord = wakeWord
            self.wakePhrase = wakePhrase
        }
    }

    /// The row a member without one reads (`defaults`).
    public static let defaults = Settings()

    /// A partial write (`voice.update`): nil = the key is absent (kept);
    /// `.some(nil)` for the device or the agent = None.
    public struct Change: Encodable, Equatable, Sendable {
        public var voiceDeviceId: String??
        public var agentId: String??
        public var speakReplies: Bool?
        public var pushToTalk: Bool?
        public var pttShortcut: String?
        public var wakeWord: Bool?
        public var wakePhrase: String?

        public init(
            voiceDeviceId: String?? = nil,
            agentId: String?? = nil,
            speakReplies: Bool? = nil,
            pushToTalk: Bool? = nil,
            pttShortcut: String? = nil,
            wakeWord: Bool? = nil,
            wakePhrase: String? = nil
        ) {
            self.voiceDeviceId = voiceDeviceId
            self.agentId = agentId
            self.speakReplies = speakReplies
            self.pushToTalk = pushToTalk
            self.pttShortcut = pttShortcut
            self.wakeWord = wakeWord
            self.wakePhrase = wakePhrase
        }

        private enum CodingKeys: String, CodingKey {
            case voiceDeviceId, agentId, speakReplies, pushToTalk, pttShortcut, wakeWord, wakePhrase
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
            try c.encodeIfPresent(speakReplies, forKey: .speakReplies)
            try c.encodeIfPresent(pushToTalk, forKey: .pushToTalk)
            try c.encodeIfPresent(pttShortcut, forKey: .pttShortcut)
            try c.encodeIfPresent(wakeWord, forKey: .wakeWord)
            try c.encodeIfPresent(wakePhrase, forKey: .wakePhrase)
        }
    }

    /// Apply a change; choosing None for the device or the agent clears every
    /// switch, so picking again never turns listening or speaking on.
    public static func clearOnNone(_ before: Settings, _ change: Change) -> Settings {
        var after = before
        if let voiceDeviceId = change.voiceDeviceId { after.voiceDeviceId = voiceDeviceId }
        if let agentId = change.agentId { after.agentId = agentId }
        if let speakReplies = change.speakReplies { after.speakReplies = speakReplies }
        if let pushToTalk = change.pushToTalk { after.pushToTalk = pushToTalk }
        if let pttShortcut = change.pttShortcut { after.pttShortcut = pttShortcut }
        if let wakeWord = change.wakeWord { after.wakeWord = wakeWord }
        if let wakePhrase = change.wakePhrase { after.wakePhrase = wakePhrase }
        if after.voiceDeviceId == nil || after.agentId == nil {
            after.speakReplies = false
            after.pushToTalk = false
            after.wakeWord = false
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
}
