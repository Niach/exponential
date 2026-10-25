import Foundation

// NEW-53 exponentialTalk, slice (b) speak replies: the Voice group of Device
// settings. A port of the TS reference `packages/domain-contract/src/voice.ts`
// (desktop `domain::voice`, Android `domain/Voice.kt`), byte-locked by
// `fixtures/voice.json` (`copy`, `testIds`, `assistantOptionsCases`, replayed
// by `VoiceTests`). The phones only show and edit the setting: speaking is
// desktop-only in this slice. The fixture's `$comment` holds the rules.

public enum Voice {
    public enum Copy {
        public static let group = "Voice"
        public static let assistant = "Assistant"
        public static let assistantNone = "None"
        public static let speakReplies = "Speak replies"
        public static let downloading = "Downloading voice…"
        public static let downloadFailed = "Couldn't download the voice"
        public static let unavailable = "The voice stopped working"
        public static let speaking = "Speaking"
        public static let stop = "Stop"
        public static let link = "link"
        public static let ellipsis = "…"
    }

    public enum TestIds {
        public static let group = "device-voice"
        public static let assistant = "device-voice-assistant"
        public static let speakReplies = "device-voice-speak-replies"
    }

    public static let maxChars = 600

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

    /// One Assistant picker option; `id` nil = no assistant.
    public struct AssistantOption: Equatable, Hashable, Sendable {
        public let id: String?
        public let label: String

        public init(id: String?, label: String) {
            self.id = id
            self.label = label
        }
    }

    /// The Assistant picker's options: None first, then the agents by label
    /// (`{name} · {team}` when two agents share a name).
    public static func assistantOptions(_ agents: [AgentLike]) -> [AssistantOption] {
        var counts: [String: Int] = [:]
        for agent in agents { counts[agent.name, default: 0] += 1 }
        let english = Locale(identifier: "en")
        let options = agents
            .map { agent in
                AssistantOption(
                    id: agent.id,
                    label: (counts[agent.name] ?? 0) > 1 ? "\(agent.name) · \(agent.team)" : agent.name
                )
            }
            .sorted { $0.label.compare($1.label, locale: english) == .orderedAscending }
        return [AssistantOption(id: nil, label: Copy.assistantNone)] + options
    }
}
