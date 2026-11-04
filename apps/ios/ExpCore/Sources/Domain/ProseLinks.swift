import Foundation

/// #EXP-1674: a bare in-app URL in agent prose reads as the chip of what it
/// names. Locked ×4 against `packages/domain-contract/fixtures/prose-links.json`,
/// whose description is the rule (web `lib/prose-links.ts`, desktop
/// `domain::prose_links`, Android `domain/ProseLinks.kt`). Display text only:
/// nothing here ever serializes.
public enum ProseLinks {
    public enum ChipKind: String, Sendable {
        case issue, board, session, agent

        /// entity-chip.json `entityRefIcon` of this kind.
        public var icon: String {
            switch self {
            case .issue: "ui-issue"
            case .board: "nav-boards"
            case .session: "coding-running"
            case .agent: "settings-agents"
            }
        }
    }

    private static let marker = "#exp-chip-"
    private static let backtick = UInt16(UInt8(ascii: "`"))
    private static let stops = Set(" \t<>()[]`\"'".utf16)
    private static let trailing = Set(".,;:!?*_~".utf16)
    private static let escaped = Set("\\[]*_`~")

    private static func isWord(_ unit: UInt16) -> Bool {
        (48...57).contains(unit) || (65...90).contains(unit) || (97...122).contains(unit) || unit == 95
    }

    private static func string(_ units: ArraySlice<UInt16>) -> String {
        String(decoding: units, as: UTF16.self)
    }

    /// The first three characters of a fence line (at most 3 leading spaces).
    private static func fenceMark(_ line: String) -> String? {
        var rest = Substring(line)
        var spaces = 0
        while spaces < 3, rest.first == " " {
            rest = rest.dropFirst()
            spaces += 1
        }
        guard rest.hasPrefix("```") || rest.hasPrefix("~~~") else { return nil }
        return String(rest.prefix(3))
    }

    private static func chip(
        _ url: String, origin: String, team: String, resolve: (ChipKind, String) -> String?
    ) -> String? {
        let named: (slug: String, kind: ChipKind, id: String)
        switch AppLink.classify(url, origin: origin, team: team) {
        case let .issue(teamSlug, _, identifier), let .issueRef(teamSlug, identifier):
            named = (teamSlug, ChipKind.issue, identifier)
        case let .board(teamSlug, boardSlug):
            named = (teamSlug, ChipKind.board, boardSlug)
        case let .session(teamSlug, sessionId):
            named = (teamSlug, ChipKind.session, sessionId)
        case let .thread(teamSlug, threadId):
            named = (teamSlug, ChipKind.session, threadId)
        case let .agent(teamSlug, agentId):
            named = (teamSlug, ChipKind.agent, agentId)
        default:
            return nil
        }
        let (slug, kind, id) = named
        guard slug.lowercased() == team.lowercased(), let title = resolve(kind, id) else { return nil }
        if kind == .issue { return "#\(id)" }
        let base = url.firstIndex(of: "#").map { String(url[..<$0]) } ?? url
        var label = ""
        for character in title {
            if escaped.contains(character) { label.append("\\") }
            label.append(character)
        }
        return "[\(label)](\(base)\(marker)\(kind.rawValue))"
    }

    private static func chipLine(
        _ line: String, origin: String, team: String, resolve: (ChipKind, String) -> String?
    ) -> String {
        let units = Array(line.utf16)
        var out = ""
        var at = 0
        var copied = 0
        while at < units.count {
            if units[at] == backtick {
                var run = at
                while run < units.count, units[run] == backtick { run += 1 }
                let width = run - at
                var close = run
                var end: Int?
                while close < units.count {
                    guard units[close] == backtick else {
                        close += 1
                        continue
                    }
                    var closeRun = close
                    while closeRun < units.count, units[closeRun] == backtick { closeRun += 1 }
                    if closeRun - close == width {
                        end = closeRun
                        break
                    }
                    close = closeRun
                }
                at = end ?? run
                continue
            }
            let head = string(units[at..<min(at + 8, units.count)]).lowercased()
            if head.hasPrefix("http://") || head.hasPrefix("https://") {
                let prev = at > 0 ? units[at - 1] : nil
                let destination = prev == UInt16(UInt8(ascii: "("))
                    && at > 1 && units[at - 2] == UInt16(UInt8(ascii: "]"))
                let glued = prev.map {
                    isWord($0) || $0 == UInt16(UInt8(ascii: "[")) || $0 == UInt16(UInt8(ascii: "<"))
                } ?? false
                if !glued, !destination {
                    var end = at
                    while end < units.count, !stops.contains(units[end]) { end += 1 }
                    while end > at, trailing.contains(units[end - 1]) { end -= 1 }
                    if let rewritten = chip(string(units[at..<end]), origin: origin, team: team, resolve: resolve) {
                        out += string(units[copied..<at])
                        out += rewritten
                        copied = end
                    }
                    at = end
                    continue
                }
            }
            at += 1
        }
        out += string(units[copied..<units.count])
        return out
    }

    /// `text` with its bare in-app URLs rewritten to their chips' sources.
    /// `resolve` = the display title of a SYNCED row, nil otherwise.
    public static func chipBareAppLinks(
        _ text: String, origin: String, team: String?, resolve: (ChipKind, String) -> String?
    ) -> String {
        guard let team, !team.isEmpty else { return text }
        let lowered = text.lowercased()
        guard lowered.contains("http://") || lowered.contains("https://") else { return text }
        var fence: String?
        return text.components(separatedBy: "\n").map { (line: String) -> String in
            if let mark = fenceMark(line) {
                if fence == nil { fence = mark } else if fence == mark { fence = nil }
                return line
            }
            return fence == nil ? chipLine(line, origin: origin, team: team, resolve: resolve) : line
        }.joined(separator: "\n")
    }

    /// The chip kind a rewritten link's href carries (nil: an ordinary link).
    public static func chipKind(ofHref href: String) -> ChipKind? {
        guard let range = href.range(of: marker, options: .backwards) else { return nil }
        switch String(href[range.upperBound...]) {
        case "board": return .board
        case "session": return .session
        case "agent": return .agent
        default: return nil
        }
    }
}
