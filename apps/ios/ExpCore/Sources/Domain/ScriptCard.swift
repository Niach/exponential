import Foundation

// EXP-1279 (EXP-1259 D13): a code-mode call (`mcp__codemode__exec`, codex
// `mcp.codemode.exec`, contract toolKind `script`) renders as ONE card — "Ran
// a script · 59 calls · 3.0 s" — with the calls it made grouped by tool, the
// script itself behind a "Script" disclosure. Pure model, hand-mirrored ×4
// (TS `packages/domain-contract/src/script-card.ts`, desktop
// `steer::script_card`, Android `domain/ScriptCard.kt`) and byte-locked by
// `packages/domain-contract/fixtures/script-card.json`, which
// `ScriptCardTests` replays.

/// One entry of code mode's call log (`tool_update.script.calls`).
public struct ScriptCall: Equatable, Sendable {
    /// `<server>.<tool>` as the script called it.
    public let tool: String
    public let subject: String?
    public let ok: Bool
    public let ms: Int?

    public init(tool: String, subject: String? = nil, ok: Bool, ms: Int? = nil) {
        self.tool = tool
        self.subject = subject
        self.ok = ok
        self.ms = ms
    }
}

/// EXP-1279 wire §1b: what a settled `script` row's `tool_update` carried —
/// the calls in order (capped at `logCap`), the TOTAL the script made, its
/// wall time and the script source. Every field is optional on the wire.
public struct AgentScriptLog: Equatable, Sendable {
    public let count: Int?
    public let elapsedMs: Int?
    public let calls: [ScriptCall]
    public let source: String?

    public init(count: Int? = nil, elapsedMs: Int? = nil, calls: [ScriptCall] = [], source: String? = nil) {
        self.count = count
        self.elapsedMs = elapsedMs
        self.calls = calls
        self.source = source
    }

    /// The wire's per-string cap on `tool`/`subject` (web `SCRIPT_TEXT_MAX`).
    static let textMax = 200

    /// Fold a wire `script` object TOLERANTLY: a malformed call is dropped,
    /// never the log; nil = nothing usable at all (web `parseScriptLog`).
    public static func parse(_ raw: Any?) -> AgentScriptLog? {
        guard let value = raw as? [String: Any] else { return nil }
        var calls: [ScriptCall] = []
        for entry in value["calls"] as? [Any] ?? [] {
            if calls.count >= ScriptCards.logCap { break }
            guard let row = entry as? [String: Any] else { continue }
            let tool = (row["tool"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !tool.isEmpty else { continue }
            let subject = (row["subject"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
            calls.append(ScriptCall(
                tool: String(tool.prefix(textMax)),
                subject: (subject?.isEmpty ?? true) ? nil : String(subject!.prefix(textMax)),
                ok: (row["ok"] as? Bool) != false,
                ms: count(row["ms"])
            ))
        }
        let total = count(value["count"])
        let elapsed = count(value["elapsedMs"])
        var source: String?
        if let text = value["source"] as? String,
           !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            source = String(text.prefix(DomainContract.steerFeedToolOutputMaxBytes))
        }
        guard !calls.isEmpty || total != nil || elapsed != nil || source != nil else { return nil }
        return AgentScriptLog(count: total, elapsedMs: elapsed, calls: calls, source: source)
    }

    /// A non-negative finite whole number off the wire, else nil.
    private static func count(_ raw: Any?) -> Int? {
        guard let number = raw as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID()
        else { return nil }
        let value = number.doubleValue
        guard value.isFinite, value >= 0, value < Double(Int.max) else { return nil }
        return Int(value.rounded())
    }
}

public struct ScriptCardInput: Equatable, Sendable {
    public let running: Bool
    /// Total calls the script made (the log is capped).
    public let count: Int?
    public let calls: [ScriptCall]?
    public let elapsedMs: Int?

    public init(running: Bool, count: Int? = nil, calls: [ScriptCall]? = nil, elapsedMs: Int? = nil) {
        self.running = running
        self.count = count
        self.calls = calls
        self.elapsedMs = elapsedMs
    }
}

public struct ScriptCardGroup: Equatable, Sendable {
    public let tool: String
    /// An Exponential tool (draw the brand mark), else the generic tool glyph.
    public let exponential: Bool
    public let label: String
    public let subject: String?
    /// `+54`, or nil.
    public let more: String?
    /// `×55`, or nil; replaced by `failed` when calls failed.
    public let count: String?
    /// `1 failed` (rose), or nil.
    public let failed: String?

    public init(
        tool: String, exponential: Bool, label: String, subject: String?, more: String?,
        count: String?, failed: String?
    ) {
        self.tool = tool
        self.exponential = exponential
        self.label = label
        self.subject = subject
        self.more = more
        self.count = count
        self.failed = failed
    }
}

public struct ScriptCard: Equatable, Sendable {
    public let title: String
    public let meta: String?
    public let groups: [ScriptCardGroup]
    /// `12 more calls`: the log's cap cut these off.
    public let overflow: String?

    public init(title: String, meta: String?, groups: [ScriptCardGroup], overflow: String?) {
        self.title = title
        self.meta = meta
        self.groups = groups
        self.overflow = overflow
    }
}

public enum ScriptCards {
    public static let separator = " · "
    /// Fixture `logCap`.
    public static let logCap = 200
    /// Fixture `toolNameSuffix` (claude's title).
    public static let toolNameSuffix = "codemode__exec"
    /// Codex titles the same call `mcp.codemode.exec` (wire doc §1b).
    public static let toolNameSuffixCodex = "codemode.exec"
    /// The contract `toolKind` a code-mode call wears.
    public static let toolKind = "script"

    /// Fixture `copy`.
    public enum Copy {
        public static let running = "Running a script"
        public static let done = "Ran a script"
        public static let script = "Script"
        public static let call = "call"
        public static let calls = "calls"
        public static let failed = "failed"
        public static let moreCall = "more call"
        public static let moreCalls = "more calls"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let card = "script-card"
        public static let group = "script-card-group"
        public static let source = "script-card-source"
    }

    /// A code-mode `exec` call: contract toolKind `script`, or (a publisher
    /// that predates the kind) a tool whose name ends in `codemode__exec`
    /// (claude) or `codemode.exec` (codex).
    public static func isScriptCall(name: String, toolKind: String?) -> Bool {
        if toolKind == Self.toolKind { return true }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.hasSuffix(toolNameSuffix) || trimmed.hasSuffix(toolNameSuffixCodex)
    }

    /// Whether a feed item is a code-mode call.
    public static func isScriptCall(_ item: AgentFeedItem) -> Bool {
        guard case let .tool(_, name, _, _, _, kind, _, _, _, _, _, _, _) = item else { return false }
        return isScriptCall(name: name, toolKind: kind)
    }

    /// `3.0 s` under a minute, the Work card's coarse format above.
    public static func duration(ms: Int) -> String {
        guard ms < 60_000 else { return WorkCards.durationShort(ms: ms) }
        return String(format: "%.1f s", locale: Locale(identifier: "en_US_POSIX"), Double(max(0, ms)) / 1000)
    }

    /// The card's input for one tool row: without a log (an older codemode)
    /// it is the bare "Ran a script".
    public static func input(settled: Bool, log: AgentScriptLog?) -> ScriptCardInput {
        ScriptCardInput(
            running: !settled, count: log?.count, calls: log?.calls, elapsedMs: log?.elapsedMs
        )
    }

    private static func plural(_ count: Int, _ one: String, _ many: String) -> String {
        "\(count) \(count == 1 ? one : many)"
    }

    public static func card(_ input: ScriptCardInput) -> ScriptCard {
        if input.running {
            return ScriptCard(title: Copy.running, meta: nil, groups: [], overflow: nil)
        }
        let calls = input.calls ?? []
        let count = max(input.count ?? calls.count, calls.count)
        var meta: [String] = []
        if count > 0 { meta.append(plural(count, Copy.call, Copy.calls)) }
        if let elapsed = input.elapsedMs { meta.append(duration(ms: elapsed)) }

        var order: [String] = []
        var byTool: [String: [ScriptCall]] = [:]
        for call in calls {
            let key = call.tool.trimmingCharacters(in: .whitespacesAndNewlines)
            if byTool[key] == nil {
                byTool[key] = []
                order.append(key)
            }
            byTool[key]?.append(call)
        }
        let groups = order.map { tool -> ScriptCardGroup in
            let members = byTool[tool] ?? []
            let name = tool.split(separator: ".", omittingEmptySubsequences: false).last.map(String.init) ?? tool
            let display = ExpToolDisplay.resolve(toolName: name)
            let subject = members
                .map { ($0.subject ?? "").trimmingCharacters(in: .whitespacesAndNewlines) }
                .first { !$0.isEmpty }
            let failed = members.filter { !$0.ok }.count
            return ScriptCardGroup(
                tool: tool,
                exponential: display != nil,
                label: display?.done ?? tool,
                subject: subject,
                more: subject != nil && members.count > 1 ? "+\(members.count - 1)" : nil,
                count: failed == 0 && members.count > 1 ? "×\(members.count)" : nil,
                failed: failed > 0 ? "\(failed) \(Copy.failed)" : nil
            )
        }
        let cut = count - calls.count
        return ScriptCard(
            title: Copy.done,
            meta: meta.isEmpty ? nil : meta.joined(separator: separator),
            groups: groups,
            overflow: cut > 0 ? "\(cut) \(cut == 1 ? Copy.moreCall : Copy.moreCalls)" : nil
        )
    }
}
