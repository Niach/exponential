import Foundation

/// One tool call as the group caption sees it. `kind` is a contract `toolKind`
/// value; `detail` is the path (or command / query) the row carries.
public struct ToolCallSummary: Equatable, Sendable {
    public let kind: String
    public let detail: String?
    public let failed: Bool

    public init(kind: String, detail: String? = nil, failed: Bool = false) {
        self.kind = kind
        self.detail = detail
        self.failed = failed
    }
}

/// EXP-785: the ONE caption a collapsed tool group renders, derived from the
/// group's tool rows. Hand-mirrored ×4 (web `@exp/domain-contract`
/// `toolGroupSummary`, Android `domain/ToolGroupSummary.kt`, desktop
/// `steer::tool_group_summary`) and byte-locked by
/// `packages/domain-contract/fixtures/tool-group-summary.json`, which every
/// client's test runs.
///
/// Rules:
/// - `kind` is a contract `toolKind` value (`read`, `edit`, `delete`, `move`,
///   `search`, `execute`, `think`, `fetch`, `switch_mode`, `other`); an unknown
///   kind counts as `other`, so an older client never chokes on a newer agent.
/// - `edit`/`delete`/`move` are "edited N files", `read` is "read N files",
///   each DEDUPED by `detail` (the path). A nil/empty detail is its own
///   distinct file every time.
/// - Fixed segment order: ran N commands · edited N files · read N files ·
///   searched N times · fetched N pages · N other tools · N failed. Zero
///   segments are omitted; `failed` counts calls with `failed` of ANY kind and
///   is always last.
/// - Only the first character of the whole caption is capitalised.
/// - Every call `think`/`switch_mode`/`other` → `Used N tools` (plus
///   ` · N failed`); no calls at all → `No tool calls`.
public enum ToolGroupSummary {
    /// The segment separator: space, MIDDLE DOT (U+00B7), space.
    public static let separator = " · "

    private static func count(_ n: Int, _ singular: String, _ plural: String? = nil) -> String {
        "\(n) \(n == 1 ? singular : (plural ?? singular + "s"))"
    }

    public static func summarize(_ calls: [ToolCallSummary]) -> String {
        if calls.isEmpty { return "No tool calls" }
        var commands = 0
        var searches = 0
        var fetches = 0
        var other = 0
        var failed = 0
        var edited = Set<String>()
        var editedBlank = 0
        var read = Set<String>()
        var readBlank = 0
        for call in calls {
            if call.failed { failed += 1 }
            let detail = call.detail.flatMap { $0.isEmpty ? nil : $0 }
            switch call.kind {
            case "execute":
                commands += 1
            case "edit", "delete", "move":
                if let path = detail { edited.insert(path) } else { editedBlank += 1 }
            case "read":
                if let path = detail { read.insert(path) } else { readBlank += 1 }
            case "search":
                searches += 1
            case "fetch":
                fetches += 1
            default:
                other += 1
            }
        }
        let editedCount = edited.count + editedBlank
        let readCount = read.count + readBlank
        var segments: [String] = []
        if commands > 0 { segments.append("ran \(count(commands, "command"))") }
        if editedCount > 0 { segments.append("edited \(count(editedCount, "file"))") }
        if readCount > 0 { segments.append("read \(count(readCount, "file"))") }
        if searches > 0 { segments.append("searched \(count(searches, "time"))") }
        if fetches > 0 { segments.append("fetched \(count(fetches, "page"))") }
        if segments.isEmpty {
            segments.append("used \(count(other, "tool"))")
        } else if other > 0 {
            segments.append(count(other, "other tool"))
        }
        if failed > 0 { segments.append("\(failed) failed") }
        let text = segments.joined(separator: separator)
        return text.prefix(1).uppercased() + text.dropFirst()
    }
}
