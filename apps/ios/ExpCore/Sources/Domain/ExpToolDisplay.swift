import Foundation

/// EXP-846 — how an **Exponential MCP tool call** renders in a transcript.
///
/// The agent calls our own MCP surface constantly (`exponential_issues_create`,
/// `exponential_pr_open`, …) and a row saying
/// `mcp__exponential__exponential_issues_create` told a reader nothing. The
/// contract's `expToolDisplay` tables name, per tool, the PROGRESSIVE caption
/// ("Creating issue"), the DONE caption ("Created issue"), which input field is
/// the subject, and what KIND of thing the answer is — so every client draws
/// the same row off the same strings.
///
/// Hand-mirrored ×4 (web `lib/agent-feed.ts`, Android `domain/ExpToolDisplay.kt`,
/// desktop `steer::exp_tool_display`); the tables themselves are GENERATED
/// (`DomainContract.expTool*`), so nothing here hard-codes a caption.
public struct ExpToolDisplay: Equatable, Sendable {
    /// The contract `expToolNames` row (`issues_create`), namespace stripped.
    public let row: String
    /// The caption while the call is in flight ("Creating issue").
    public let progressive: String
    /// The caption once it settled ("Created issue").
    public let done: String
    /// The INPUT field that names the subject (`title`, `id`, …); empty when
    /// the tool takes no subject (a list).
    public let subjectKey: String
    /// What the answer IS — what the row previews on completion.
    public let result: ExpToolResultKind

    public init(
        row: String,
        progressive: String,
        done: String,
        subjectKey: String,
        result: ExpToolResultKind
    ) {
        self.row = row
        self.progressive = progressive
        self.done = done
        self.subjectKey = subjectKey
        self.result = result
    }

    /// The caption a row wears: progressive while running, done once settled.
    public func caption(settled: Bool) -> String { settled ? done : progressive }

    /// The Exponential tool a call NAMES, or nil for every other tool.
    ///
    /// A name matches when it ends with a contract row AND the part in front of
    /// it ends with the contract prefix — so `exponential_issues_create`,
    /// `mcp__exponential__exponential_issues_create` and
    /// `exponential.exponential_issues_create` all resolve, while another
    /// server's `issues_create` (no `exponential_` in front) never does.
    /// Mirrors desktop `mapper::exp_tool_row`.
    public static func resolve(toolName: String) -> ExpToolDisplay? {
        let name = toolName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        let rows = DomainContract.expToolNames
        guard let index = rows.firstIndex(where: { row in
            guard !row.isEmpty, name.count > row.count, name.hasSuffix(row) else { return false }
            return name.dropLast(row.count).hasSuffix(DomainContract.expToolPrefix)
        }) else { return nil }
        // The five tables are generated from ONE contract list, so they are the
        // same length — the guard is a belt against a half-regenerated client,
        // which must render a plain tool row rather than crash.
        guard DomainContract.expToolProgressive.indices.contains(index),
              DomainContract.expToolDone.indices.contains(index),
              DomainContract.expToolSubjectKeys.indices.contains(index),
              DomainContract.expToolResults.indices.contains(index)
        else { return nil }
        return ExpToolDisplay(
            row: rows[index],
            progressive: DomainContract.expToolProgressive[index],
            done: DomainContract.expToolDone[index],
            subjectKey: DomainContract.expToolSubjectKeys[index],
            result: ExpToolResultKind(wire: DomainContract.expToolResults[index])
        )
    }
}

/// EXP-846: the contract `expToolDisplay.resultKinds` — what an Exponential
/// tool's answer IS, and therefore what its row previews. An unknown value
/// (a newer contract against an older client) reads as `.none`: a caption
/// without a preview, never a broken row.
public enum ExpToolResultKind: String, Sendable, CaseIterable {
    case none
    case issue
    case pr
    case comment
    case session
    case board
    case action
    case automation
    case list

    public init(wire: String) {
        // Spelled out rather than `?? .none`: a `none` CASE beside Optional's
        // own `none` is exactly where Swift's inference gets interesting.
        if let kind = ExpToolResultKind(rawValue: wire) {
            self = kind
        } else {
            self = ExpToolResultKind.none
        }
    }
}
