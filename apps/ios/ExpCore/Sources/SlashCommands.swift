import Foundation

// EXP-724 — the curated steer slash-command catalog and its two matching
// rules, byte-identical on all four clients (web `lib/steer-commands.ts`,
// Android `domain/SlashCommands.kt`, desktop `ui/src/slash_commands.rs`).
// Foundation only (ExpCore rule): the composer's menu and the confirm dialog
// are pure functions of the draft plus the session's agent, so ExpCoreTests
// drives every rule without a view.
//
// Commands ride the ORDINARY input frames as text (`/name args` + `\r`) — the
// desktop recognizes a catalog command by its first token and executes it per
// agent. There is no new relay frame, and nothing here decides what a command
// DOES.

/// One catalog row, zipped out of the generated contract arrays.
public struct SlashCommand: Equatable, Sendable, Identifiable {
    /// Bare name, no leading slash (`compact`).
    public let name: String
    public let description: String
    /// What the argument means, for the menu's muted hint. Empty = the command
    /// takes none, which is also what decides whether accepting it leaves a
    /// trailing space.
    public let argHint: String
    /// The agents that can run it — `coding_sessions.agent` values.
    public let agents: [String]
    /// Destructive enough to need a confirm before it goes out (`/clear`,
    /// the conversation is discarded).
    public let confirm: Bool

    public var id: String { name }

    public init(name: String, description: String, argHint: String, agents: [String], confirm: Bool) {
        self.name = name
        self.description = description
        self.argHint = argHint
        self.agents = agents
        self.confirm = confirm
    }

    /// What accepting this row puts in the composer: a trailing space when the
    /// command takes an argument (the caret lands where the argument goes),
    /// nothing extra when it does not. Accepting NEVER sends.
    public var insertion: String {
        argHint.isEmpty ? "/\(name)" : "/\(name) "
    }

    /// The token a sent message must open with to BE this command.
    public var token: String { "/\(name)" }
}

public enum SlashCommands {
    /// The whole catalog in contract order — the order the menu lists in.
    public static let all: [SlashCommand] = {
        let names = DomainContract.steerCommandNames
        let descriptions = DomainContract.steerCommandDescriptions
        let argHints = DomainContract.steerCommandArgHints
        let agents = DomainContract.steerCommandAgents
        let confirm = DomainContract.steerCommandConfirm
        return names.indices.compactMap { i -> SlashCommand? in
            guard i < descriptions.count, i < argHints.count,
                  i < agents.count, i < confirm.count else { return nil }
            return SlashCommand(
                name: names[i],
                description: descriptions[i],
                argHint: argHints[i],
                // The generated array carries one comma-joined string per row
                // (the contract generator has no nested-array emitter).
                agents: agents[i].split(separator: ",").map(String.init),
                confirm: confirm[i]
            )
        }
    }()

    /// The agent of a session that reports none — a pre-EXP-201 row, or a
    /// desktop old enough not to stamp it.
    public static var defaultAgent: String { DomainContract.codingAgentValues[0] }

    /// EXP-746: the catalog id of an EXTERNAL ACP agent. Deliberately a value
    /// contract `codingAgent` (and so `steerCommands`) cannot name, so its
    /// curated catalog is EMPTY — the desktop's
    /// `steer::commands::agent_id(SessionAgent::External)`.
    public static let externalAgent = "external"

    /// EXP-746: which catalog a session's `/` menu and command pills key off.
    ///
    /// A row that names no agent is normally a claude run that predates the
    /// column — but an EXTERNAL agent syncs no agent EITHER, because
    /// `coding_sessions.agent` takes contract values only and there is none
    /// for one. `acp` tells them apart: only the ACP engine publishes a
    /// `config_state`, and an ACP run for a contract agent always stamps its
    /// id. Without this a phone offered `/compact` and `/clear` — confirm
    /// dialog and all — for a run whose desktop-side catalog is empty, and
    /// the literal text reached the agent as a prompt. Mirrored ×4 (web
    /// `steerAgentId`, Android `SlashCommands.agentId`, desktop
    /// `slash_commands::agent_of`).
    public static func agentId(_ agent: String?, acp: Bool) -> String {
        let trimmed = agent?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !trimmed.isEmpty { return trimmed }
        return acp ? externalAgent : defaultAgent
    }

    /// Every command the given agent can run, in catalog order.
    public static func catalog(for agent: String?) -> [SlashCommand] {
        let resolved = resolve(agent)
        return all.filter { $0.agents.contains(resolved) }
    }

    /// The menu's rows for a draft, or empty when the menu must not open.
    ///
    /// The menu opens iff the WHOLE draft is a leading slash plus a partial
    /// name — `^/[A-Za-z0-9-]*$`. So it opens on `/`, stays open through
    /// `/co`, and closes the moment an argument starts (`/compact `) or the
    /// slash is not at position 0 (`x /c`). The filter is a case-insensitive
    /// name PREFIX; an empty query lists everything the agent can run.
    public static func matches(draft: String, agent: String?) -> [SlashCommand] {
        guard let query = partialName(in: draft) else { return [] }
        let needle = query.lowercased()
        return catalog(for: agent).filter {
            needle.isEmpty || $0.name.lowercased().hasPrefix(needle)
        }
    }

    /// EXP-746: the `/` menu's catalog for a session whose agent advertises
    /// commands of its own (`config_state.commands`, ACP
    /// `available_commands_update`): the CONTRACT rows first, then the agent's
    /// extras, deduped by lowercased name — a contract name always wins, so an
    /// agent's `/compact` can never shadow the curated row and its confirm
    /// copy. `contract` is expected to be agent-filtered already
    /// (`catalog(for:)`); the extras came off THIS session's agent, so they
    /// carry no `agents` of their own and never leak into another agent's
    /// catalog. They also carry `confirm: false`: nothing an agent advertises
    /// is destructive enough for us to promise a dialog for it.
    public static func merged(
        _ contract: [SlashCommand], agent commands: [AgentConfigCommand]
    ) -> [SlashCommand] {
        var seen = Set(contract.map { $0.name.lowercased() })
        var merged = contract
        for command in commands {
            let name = command.name.trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            guard !name.isEmpty, seen.insert(name.lowercased()).inserted else { continue }
            merged.append(SlashCommand(
                name: name,
                description: command.description,
                argHint: command.hint ?? "",
                agents: [],
                confirm: false
            ))
        }
        return merged
    }

    /// `matches(draft:agent:)` over the merged catalog (EXP-746).
    public static func matches(
        draft: String, agent: String?, extra: [AgentConfigCommand]
    ) -> [SlashCommand] {
        guard let query = partialName(in: draft) else { return [] }
        let needle = query.lowercased()
        return merged(catalog(for: agent), agent: extra).filter {
            needle.isEmpty || $0.name.lowercased().hasPrefix(needle)
        }
    }

    /// The command a message about to be sent IS, if any: its first
    /// whitespace-separated token has to equal `/name` exactly
    /// (case-insensitively) and the command has to be runnable by this agent.
    /// Anything else — `/compactify`, `hello /compact`, `//compact` — is
    /// ordinary prose and goes out untouched.
    public static func command(for text: String, agent: String?) -> SlashCommand? {
        command(for: text, agent: agent, extra: [])
    }

    /// EXP-746: `command(for:agent:)` over the MERGED catalog — a command the
    /// agent advertised in `config_state` is a command here too, so a steered
    /// `/review` comes back into the transcript as a command pill instead of a
    /// prose bubble. The menu and the lookup read one catalog, exactly as they
    /// do on web (`parseSteerCommand` over `mergeAgentCommands`) and Android
    /// (`SlashCommands.commandFor(text, agent, agentCommands)`). The matching
    /// rules are unchanged: still the first whitespace token, still exact and
    /// case-insensitive, and an empty `extra` is the contract catalog.
    public static func command(
        for text: String, agent: String?, extra: [AgentConfigCommand]
    ) -> SlashCommand? {
        guard let token = text.split(whereSeparator: { $0.isWhitespace }).first else { return nil }
        let needle = token.lowercased()
        return merged(catalog(for: agent), agent: extra).first { $0.token.lowercased() == needle }
    }

    // MARK: - Confirm dialog copy (byte-identical ×4)

    public static func confirmTitle(_ command: SlashCommand) -> String {
        "Run /\(command.name)?"
    }

    public static let confirmBody =
        "The agent forgets everything in this session so far. Files in the worktree are kept."

    public static func confirmButton(_ command: SlashCommand) -> String {
        "Run /\(command.name)"
    }

    // MARK: - Internals

    private static func resolve(_ agent: String?) -> String {
        let trimmed = agent?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? defaultAgent : trimmed
    }

    /// The partial name a draft carries, or nil when the draft is not a
    /// menu-opening draft at all. `"/"` yields `""` (list everything).
    private static func partialName(in draft: String) -> String? {
        guard draft.hasPrefix("/") else { return nil }
        let rest = draft.dropFirst()
        for character in rest {
            guard character.isASCII,
                  character.isLetter || character.isNumber || character == "-"
            else { return nil }
        }
        return String(rest)
    }
}
