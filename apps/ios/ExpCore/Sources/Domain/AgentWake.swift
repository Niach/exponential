import Foundation

// #EXP-1859 (agent-settings.json `voice.wake`, phase d of #EXP-1847): wake
// phrases per agent. The member's voice device listens for the phrases of
// `wakeListeners` (the voice agent first, then the team agents the member
// switched on); a TEAM agent's page carries the member's `wake-word` row
// (`wakeRow`), a flip goes through `wakeToggle` (the server applies the same
// rule in `voice.wakeAgent`). Phones have no wake word of their own: the row
// chooses who listens on the voice device.
extension AgentSettings {
    public enum Wake {
        /// The row's id in `VoiceRows.rows` (team agents' pages only).
        public static let rowId = "wake-word"
        public static let maxListeners = 5
        public static let maxIds = 32
        /// talk-listen.json `wake.constants.maxEditRatio`.
        public static let maxEditRatio = 0.25

        public enum Copy {
            public static let wakeWord = "Wake word"
            public enum Errors {
                public static let wakeTooShort = "The wake phrase needs at least 6 letters"
                public static let wakeClash = "Sounds like {agent}'s wake phrase"
                public static let wakeCap = "At most 5 agents listen at once"
            }

            public static func wakeClash(_ agentName: String) -> String {
                Errors.wakeClash.replacingOccurrences(of: "{agent}", with: agentName)
            }
        }
    }

    /// An agent as synced (the fields the wake rules read).
    public struct WakeAgent: Equatable, Sendable {
        public let id: String
        public let role: String
        public let name: String
        public let voice: AgentVoice

        public init(id: String, role: String, name: String, voice: AgentVoice) {
            self.id = id
            self.role = role
            self.name = name
            self.voice = voice
        }

        /// A team agent (role agent or coordinator), never an Assistant.
        public var isTeamAgent: Bool {
            role == DomainContract.agentRoleAgent || role == DomainContract.agentRoleCoordinator
        }
    }

    public struct WakeListener: Equatable, Sendable {
        public let agentId: String
        public let phrase: String
        public let language: String

        public init(agentId: String, phrase: String, language: String) {
            self.agentId = agentId
            self.phrase = phrase
            self.language = language
        }
    }

    public struct WakeRowState: Equatable, Sendable {
        public let on: Bool
        /// Why an agent that is on does not listen (nil = none).
        public let error: String?

        public init(on: Bool, error: String?) {
            self.on = on
            self.error = error
        }
    }

    public struct WakeToggleResult: Equatable, Sendable {
        public let wakeWord: Bool
        public let wakeAgentIds: [String]
        /// The refusal's copy (nil = applied).
        public let error: String?

        public init(wakeWord: Bool, wakeAgentIds: [String], error: String?) {
            self.wakeWord = wakeWord
            self.wakeAgentIds = wakeAgentIds
            self.error = error
        }
    }

    /// talk-listen.json `wake.words` joined without spaces: runs of letters
    /// and digits, lower case, accents folded (ä→a, ß→ss, é→e …).
    public static func wakeLetters(_ phrase: String) -> [Character] {
        let folded = phrase.lowercased()
            .replacingOccurrences(of: "ß", with: "ss")
            .folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
            .lowercased()
        return folded.filter { $0.isLetter || $0.isNumber }.map { $0 }
    }

    static func levenshtein(_ a: [Character], _ b: [Character]) -> Int {
        if a.isEmpty { return b.count }
        if b.isEmpty { return a.count }
        var previous = Array(0...b.count)
        var current = [Int](repeating: 0, count: b.count + 1)
        for i in 1...a.count {
            current[0] = i
            for j in 1...b.count {
                let cost = a[i - 1] == b[j - 1] ? 0 : 1
                current[j] = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost)
            }
            swap(&previous, &current)
        }
        return previous[b.count]
    }

    /// `wakeClash` (`clashCases`): alike enough for the matcher to take one
    /// for the other, or one starting with the other.
    public static func wakeClash(_ a: String, _ b: String) -> Bool {
        let la = wakeLetters(a)
        let lb = wakeLetters(b)
        let (s, l) = la.count <= lb.count ? (la, lb) : (lb, la)
        let distance = min(levenshtein(s, l), levenshtein(s, Array(l.prefix(s.count))))
        return distance <= Int((Double(s.count) * Wake.maxEditRatio).rounded(.down))
    }

    /// An agent's wake phrase: `agentVoiceOf`'s, else `Hey {name}`.
    public static func wakePhraseOf(_ agent: WakeAgent, member: Voice.Settings) -> String {
        let phrase = agentVoiceOf(role: agent.role, voice: agent.voice, member: member).wakePhrase
        return phrase.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? VoiceRows.wakePhrasePlaceholder(agent.name)
            : phrase
    }

    /// `wakeListeners` (`listenersCases`): nobody without Wake word, a voice
    /// device or a voice agent; else the voice agent, then the switched-on
    /// team agents in order (skipping the unknown, Assistants, repeats, too
    /// short or clashing phrases), at most `maxListeners`.
    public static func wakeListeners(_ settings: Voice.Settings, agents: [WakeAgent]) -> [WakeListener] {
        listeners(settings, agents: agents, ids: settings.wakeAgentIds, leaving: nil)
    }

    private static func listeners(
        _ settings: Voice.Settings, agents: [WakeAgent], ids: [String], leaving: String?
    ) -> [WakeListener] {
        guard settings.wakeWord, settings.voiceDeviceId != nil, let voiceAgentId = settings.agentId else { return [] }
        var out: [WakeListener] = []
        func listener(_ agent: WakeAgent) -> WakeListener {
            WakeListener(
                agentId: agent.id,
                phrase: wakePhraseOf(agent, member: settings),
                language: agentVoiceOf(role: agent.role, voice: agent.voice, member: settings).language
            )
        }
        if voiceAgentId != leaving, let voiceAgent = agents.first(where: { $0.id == voiceAgentId }) {
            out.append(listener(voiceAgent))
        }
        for id in ids where id != leaving {
            guard out.count < Wake.maxListeners else { break }
            guard let agent = agents.first(where: { $0.id == id }), agent.isTeamAgent,
                  !out.contains(where: { $0.agentId == id })
            else { continue }
            let next = listener(agent)
            if Voice.wakePhraseError(next.phrase) != nil { continue }
            if out.contains(where: { wakeClash($0.phrase, next.phrase) }) { continue }
            out.append(next)
        }
        return out
    }

    private static func name(_ agentId: String, _ agents: [WakeAgent]) -> String {
        agents.first { $0.id == agentId }?.name ?? ""
    }

    /// The `wake-word` row (`rowCases`): on = Wake word on and the id
    /// switched on; the error says why an agent that is on does not listen.
    public static func wakeRow(_ settings: Voice.Settings, agents: [WakeAgent], agentId: String) -> WakeRowState {
        let on = settings.wakeWord && settings.wakeAgentIds.contains(agentId)
        guard on else { return WakeRowState(on: false, error: nil) }
        if wakeListeners(settings, agents: agents).contains(where: { $0.agentId == agentId }) {
            return WakeRowState(on: true, error: nil)
        }
        guard let agent = agents.first(where: { $0.id == agentId }), agent.isTeamAgent else {
            return WakeRowState(on: true, error: nil)
        }
        let phrase = wakePhraseOf(agent, member: settings)
        if Voice.wakePhraseError(phrase) != nil {
            return WakeRowState(on: true, error: Wake.Copy.Errors.wakeTooShort)
        }
        let before = Array(settings.wakeAgentIds.prefix(while: { $0 != agentId }))
        if let clash = listeners(settings, agents: agents, ids: before, leaving: nil)
            .first(where: { wakeClash($0.phrase, phrase) }) {
            return WakeRowState(on: true, error: Wake.Copy.wakeClash(name(clash.agentId, agents)))
        }
        return WakeRowState(on: true, error: Wake.Copy.Errors.wakeCap)
    }

    /// A flip (`toggleCases`): off = the id leaves (Wake word unchanged); on
    /// = refused when too short, clashing or over the cap (counted as if
    /// Wake word were on, the agent itself left out), else appended once and
    /// Wake word turns on. An agent that is not a team agent changes nothing
    /// (the server answers NOT_FOUND).
    public static func wakeToggle(
        _ settings: Voice.Settings, agents: [WakeAgent], agentId: String, on: Bool
    ) -> WakeToggleResult {
        guard on else {
            return WakeToggleResult(
                wakeWord: settings.wakeWord, wakeAgentIds: settings.wakeAgentIds.filter { $0 != agentId }, error: nil
            )
        }
        let unchanged = { (error: String?) in
            WakeToggleResult(wakeWord: settings.wakeWord, wakeAgentIds: settings.wakeAgentIds, error: error)
        }
        guard let agent = agents.first(where: { $0.id == agentId }), agent.isTeamAgent else { return unchanged(nil) }
        let phrase = wakePhraseOf(agent, member: settings)
        if Voice.wakePhraseError(phrase) != nil { return unchanged(Wake.Copy.Errors.wakeTooShort) }
        var armed = settings
        armed.wakeWord = true
        let listening = listeners(armed, agents: agents, ids: settings.wakeAgentIds, leaving: agentId)
        if let clash = listening.first(where: { wakeClash($0.phrase, phrase) }) {
            return unchanged(Wake.Copy.wakeClash(name(clash.agentId, agents)))
        }
        if listening.count >= Wake.maxListeners { return unchanged(Wake.Copy.Errors.wakeCap) }
        let ids = settings.wakeAgentIds.contains(agentId) ? settings.wakeAgentIds : settings.wakeAgentIds + [agentId]
        return WakeToggleResult(wakeWord: true, wakeAgentIds: ids, error: nil)
    }
}
