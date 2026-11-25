import Foundation

/// #EXP-1847 (agent-settings.json `voice`): an agent's OWN voice, the synced
/// `agents.voice` jsonb (`{}` = the member's). Encodes as one JSON object
/// (GRDB stores it as JSON text); a nil key is omitted.
public struct AgentVoice: Codable, Equatable, Hashable, Sendable {
    public var language: String?
    public var clipId: String?
    public var clipName: String?
    public var transcript: String?
    public var wakePhrase: String?

    public init(
        language: String? = nil,
        clipId: String? = nil,
        clipName: String? = nil,
        transcript: String? = nil,
        wakePhrase: String? = nil
    ) {
        self.language = language
        self.clipId = clipId
        self.clipName = clipName
        self.transcript = transcript
        self.wakePhrase = wakePhrase
    }

    /// The jsonb's JSON text; nil, garbage or a non-object = `{}`. A key of
    /// the wrong type reads as unset.
    public static func parse(_ raw: String?) -> AgentVoice {
        guard let raw, let data = raw.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return AgentVoice() }
        func text(_ key: String) -> String? { object[key] as? String }
        return AgentVoice(
            language: text("language"),
            clipId: text("clipId"),
            clipName: text("clipName"),
            transcript: text("transcript"),
            wakePhrase: text("wakePhrase")
        )
    }
}

/// `agents.update {voice}`: nil = the key is absent (kept); `.some(nil)` =
/// an explicit null (cleared). The clip is written only by its route.
public struct AgentVoicePatch: Encodable, Equatable, Sendable {
    public var language: String??
    public var transcript: String??
    public var wakePhrase: String??

    public init(language: String?? = nil, transcript: String?? = nil, wakePhrase: String?? = nil) {
        self.language = language
        self.transcript = transcript
        self.wakePhrase = wakePhrase
    }

    public var isEmpty: Bool { language == nil && transcript == nil && wakePhrase == nil }

    enum CodingKeys: String, CodingKey { case language, transcript, wakePhrase }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let language { try c.encode(language, forKey: .language) }
        if let transcript { try c.encode(transcript, forKey: .transcript) }
        if let wakePhrase { try c.encode(wakePhrase, forKey: .wakePhrase) }
    }
}

extension AgentSettings {
    /// Fixture `voice`: the Voice section every agent's page carries.
    public enum VoiceRows {
        /// #EXP-1859: `wake-word` on a TEAM agent's page only (`Wake.rowId`).
        public static let rows = ["language", "clip", "transcript", "wake-phrase", "wake-word"]

        public enum Copy {
            public static let label = "Voice"
            public static let language = "Language"
            public static let clip = "Voice"
            public static let clipBuiltIn = "Built-in"
            public static let clipUpload = "Upload a voice clip"
            public static let clipRemove = "Remove clip"
            public static let transcript = "Transcript"
            public static let transcriptPlaceholder = "What the clip says"
            public static let wakePhrase = "Wake phrase"
            public static let wakePhrasePlaceholder = "Hey {agent}"
            public enum Errors {
                public static let clipType = "Voice clips must be WAV files"
                public static let clipSize = "Voice clips must be 5 MB or smaller"
            }
        }

        public enum ClipUpload {
            public static let route = "/api/agents/{agentId}/voice-clip"
            public static let accept = ["audio/wav", "audio/x-wav", "audio/wave"]
            public static let maxBytes = 5_242_880
        }

        public enum Limits {
            public static let transcript = 500
            public static let wakePhrase = 64
            public static let wakePhraseMinLetters = 6
        }

        /// The clip route of an agent.
        public static func clipPath(agentId: String) -> String {
            ClipUpload.route.replacingOccurrences(of: "{agentId}", with: agentId)
        }

        /// The wake phrase's placeholder for an agent.
        public static func wakePhrasePlaceholder(_ agentName: String) -> String {
            Copy.wakePhrasePlaceholder.replacingOccurrences(of: "{agent}", with: agentName)
        }
    }

    /// What the voice an agent speaks with is read from (`voiceOfCases`).
    public enum ResolvedClip: Equatable, Sendable {
        case attachment(id: String)
        case path(String)
    }

    public struct ResolvedVoice: Equatable, Sendable {
        public let language: String
        /// nil = the built-in voice.
        public let clip: ResolvedClip?
        public let clipLabel: String
        public let transcript: String
        public let wakePhrase: String

        public init(language: String, clip: ResolvedClip?, clipLabel: String, transcript: String, wakePhrase: String) {
            self.language = language
            self.clip = clip
            self.clipLabel = clipLabel
            self.transcript = transcript
            self.wakePhrase = wakePhrase
        }
    }

    /// The voice an agent speaks with (TS `agentVoiceOf`, `voiceOfCases`):
    /// its own keys first, then (an Assistant only) the member's voice file
    /// and wake phrase; the language always falls back to the member's.
    public static func agentVoiceOf(role: String?, voice: AgentVoice, member: Voice.Settings) -> ResolvedVoice {
        let assistant = role == DomainContract.agentRoleAssistant
        let language = voice.language.flatMap { $0.isEmpty ? nil : $0 } ?? member.language
        let ownPhrase = voice.wakePhrase.flatMap {
            $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
        }
        let wakePhrase = ownPhrase ?? (assistant ? member.wakePhrase : "")
        if let clipId = voice.clipId, !clipId.isEmpty {
            let name = voice.clipName ?? ""
            return ResolvedVoice(
                language: language,
                clip: .attachment(id: clipId),
                clipLabel: name.isEmpty ? VoiceRows.Copy.clipBuiltIn : name,
                transcript: voice.transcript ?? "",
                wakePhrase: wakePhrase
            )
        }
        if assistant, !member.voiceFile.isEmpty {
            let last = member.voiceFile.split(omittingEmptySubsequences: false, whereSeparator: { $0 == "/" || $0 == "\\" })
                .last.map(String.init) ?? ""
            return ResolvedVoice(
                language: language,
                clip: .path(member.voiceFile),
                clipLabel: last.isEmpty ? member.voiceFile : last,
                transcript: voice.transcript ?? member.voiceTranscript,
                wakePhrase: wakePhrase
            )
        }
        return ResolvedVoice(
            language: language, clip: nil, clipLabel: VoiceRows.Copy.clipBuiltIn, transcript: voice.transcript ?? "", wakePhrase: wakePhrase
        )
    }
}
