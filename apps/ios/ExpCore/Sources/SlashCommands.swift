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
    /// `/new`: the conversation is discarded).
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

    /// The command a message about to be sent IS, if any: its first
    /// whitespace-separated token has to equal `/name` exactly
    /// (case-insensitively) and the command has to be runnable by this agent.
    /// Anything else — `/compactify`, `hello /compact`, `//compact` — is
    /// ordinary prose and goes out untouched.
    public static func command(for text: String, agent: String?) -> SlashCommand? {
        guard let token = text.split(whereSeparator: { $0.isWhitespace }).first else { return nil }
        let needle = token.lowercased()
        return catalog(for: agent).first { $0.token.lowercased() == needle }
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
