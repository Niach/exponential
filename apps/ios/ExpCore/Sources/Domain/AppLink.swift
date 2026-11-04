import Foundation

/// EXP-1188 — what a tapped markdown link in agent prose (narration, Guide,
/// thread) opens. Locked ×4 against `packages/domain-contract/fixtures/app-link.json`
/// (web `lib/app-link.ts`, desktop `domain::app_link`, Android `domain/AppLink.kt`);
/// the fixture's description is the rule. Hand-parsed on purpose: `URL` and
/// `URLComponents` normalise, reject and percent-encode differently from the
/// other three clients, and the point is ONE answer.
public enum AppLink: Equatable, Sendable {
    case issue(teamSlug: String, boardSlug: String, identifier: String)
    /// #EXP-1674: an issue by identifier alone (`/t/<team>/issues/<identifier>`).
    case issueRef(teamSlug: String, identifier: String)
    case board(teamSlug: String, boardSlug: String)
    case session(teamSlug: String, sessionId: String)
    case plan(teamSlug: String, sessionId: String, messageId: String)
    /// EXP-1284: a thread on this instance.
    case thread(teamSlug: String, threadId: String)
    /// The chats list.
    case threads(teamSlug: String)
    case agent(teamSlug: String, agentId: String)
    /// The team's agent page (`/t/<team>/agent`).
    case agentHome(teamSlug: String)
    case inbox(teamSlug: String)
    case devices(teamSlug: String)
    /// `section` = the segment, "" for the Settings root.
    case settings(teamSlug: String, section: String)
    /// The app's first screen in THE RUN VIEW (apps.json).
    case appHome(teamSlug: String, appSlug: String)
    /// VAPP-218: one screen of a team app (`/t/<team>/apps/<app>/<screen>`),
    /// its query = the screen's params: THE RUN VIEW showing that screen.
    case appScreen(teamSlug: String, appSlug: String, screen: String, params: [String: String])
    /// Another page on this instance, the path as written (query + hash kept).
    case app(path: String)
    /// Open as given (another origin, `mailto:`).
    case external(url: String)
    /// Nothing to open: placeholder hosts, `javascript:`, `//x`, `#x`, words.
    case ignore

    /// `copy.noAccess` of the fixture: the toast of a link that cannot open.
    public static let noAccessCopy = "Not found or no access"

    /// `team` = the SURFACE'S team slug (of the message, issue or app holding
    /// the link); nil = none, so the `~/` shorthand opens nothing.
    public static func classify(_ href: String, origin: String, team: String? = nil) -> AppLink {
        let trimmed = href.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return .ignore }
        if trimmed.hasPrefix("~") {
            guard let expanded = expandTeamLink(trimmed, team: team) else { return .ignore }
            return inApp(path: expanded)
        }
        if trimmed.hasPrefix("/") {
            if trimmed.hasPrefix("//") { return .ignore }
            return inApp(path: trimmed)
        }
        let lower = trimmed.lowercased()
        if lower.hasPrefix("mailto:") { return .external(url: trimmed) }
        let scheme: String
        if lower.hasPrefix("https://") {
            scheme = "https://"
        } else if lower.hasPrefix("http://") {
            scheme = "http://"
        } else {
            return .ignore
        }
        let rest = trimmed.dropFirst(scheme.count)
        let authorityEnd = rest.firstIndex(where: { $0 == "/" || $0 == "?" || $0 == "#" }) ?? rest.endIndex
        let authority = String(rest[rest.startIndex..<authorityEnd])
        guard isValidAuthority(authority) else { return .ignore }
        let normalizedOrigin = normalized(origin)
        if !normalizedOrigin.isEmpty, (scheme + authority).lowercased() == normalizedOrigin {
            return inApp(path: String(rest[authorityEnd...]))
        }
        return .external(url: trimmed)
    }

    /// #EXP-1674: `~/x` = `/t/<team>/x`, or nil when it cannot be expanded (no
    /// team, not `~/`, `~//`, a `.` or `..` segment even percent-encoded).
    public static func expandTeamLink(_ href: String, team: String?) -> String? {
        guard let team, !team.isEmpty, href.hasPrefix("~/"), !href.hasPrefix("~//") else { return nil }
        let rest = String(href.dropFirst(2))
        let pathOnly = rest.prefix(while: { $0 != "?" && $0 != "#" })
        let hasDotSegment = pathOnly.split(separator: "/", omittingEmptySubsequences: false).contains { segment in
            let value = decoded(String(segment))
            return value == "." || value == ".."
        }
        if hasDotSegment { return nil }
        let encoded = team.addingPercentEncoding(withAllowedCharacters: uriComponentAllowed) ?? team
        return "/t/\(encoded)/\(rest)"
    }

    /// The characters `encodeURIComponent` leaves alone.
    private static let uriComponentAllowed = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()"
    )

    /// #EXP-1674: what opening a resolved in-app link does, the ONE rule ×4
    /// (`openCases`). Unreadable looks exactly like missing.
    public static func linkOpenOutcome(member: Bool, activeTeam: Bool, found: Bool, webOnly: Bool) -> LinkOpenOutcome {
        if !member { return .noAccess }
        if webOnly { return .browser }
        if !found { return .notFound }
        return activeTeam ? .open : .switchTeam
    }

    /// The team an in-app link lands in: its route's, or for an `app` path
    /// the `/t/<team>/…` it is written under. Nil = no team (`/admin`, the
    /// root) or not an in-app link.
    public var teamSlug: String? {
        switch self {
        case let .issue(team, _, _), let .issueRef(team, _), let .board(team, _),
             let .session(team, _), let .plan(team, _, _), let .thread(team, _),
             let .threads(team), let .agent(team, _), let .agentHome(team),
             let .inbox(team), let .devices(team), let .settings(team, _),
             let .appHome(team, _), let .appScreen(team, _, _, _):
            return team
        case let .app(path):
            let parts = Self.segments(path)
            return parts.count >= 2 && parts[0] == "t" ? parts[1] : nil
        case .external, .ignore:
            return nil
        }
    }

    /// `[A-Za-z0-9-]` labels joined by `.` (two or more, or `localhost`), an
    /// optional `:port`, no userinfo.
    private static func isValidAuthority(_ authority: String) -> Bool {
        var host = Substring(authority)
        if let colon = authority.firstIndex(of: ":") {
            let port = authority[authority.index(after: colon)...]
            guard !port.isEmpty, port.allSatisfy({ $0.isASCII && $0.isNumber }) else { return false }
            host = authority[authority.startIndex..<colon]
        }
        guard !host.isEmpty else { return false }
        let labels = host.split(separator: ".", omittingEmptySubsequences: false)
        // Two labels at least (or `localhost`): a browser punycodes the
        // placeholder `https://…` into the one-label host `xn--rvg`.
        guard labels.count >= 2 || host.lowercased() == "localhost" else { return false }
        return labels.allSatisfy { label in
            !label.isEmpty && label.unicodeScalars.allSatisfy { scalar in
                (scalar >= "a" && scalar <= "z") || (scalar >= "A" && scalar <= "Z")
                    || (scalar >= "0" && scalar <= "9") || scalar == "-"
            }
        }
    }

    private static func normalized(_ origin: String) -> String {
        var value = origin.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        while value.hasSuffix("/") { value.removeLast() }
        return value
    }

    /// A path's segments: query + hash dropped, empty ones dropped, each
    /// percent-decoded.
    private static func segments(_ path: String) -> [String] {
        path.prefix(while: { $0 != "?" && $0 != "#" })
            .split(separator: "/")
            .map { decoded(String($0)) }
    }

    /// THE ROUTES (`routes`, exact segment counts; anything else stays `app`).
    /// `path` = everything after the authority (may be empty or start at `?`/`#`).
    private static func inApp(path: String) -> AppLink {
        let written = path.hasPrefix("/") ? path : "/" + path
        let parts = segments(written)
        guard parts.count >= 3, parts[0] == "t" else { return .app(path: written) }
        let team = parts[1]
        let count = parts.count
        switch parts[2] {
        case "issues" where count == 4:
            return .issueRef(teamSlug: team, identifier: parts[3])
        case "boards" where count == 6 && parts[4] == "issues":
            return .issue(teamSlug: team, boardSlug: parts[3], identifier: parts[5])
        case "boards" where count == 4:
            return .board(teamSlug: team, boardSlug: parts[3])
        case "sessions" where count == 6 && parts[4] == "plan":
            return .plan(teamSlug: team, sessionId: parts[3], messageId: parts[5])
        case "sessions" where count == 4:
            return .session(teamSlug: team, sessionId: parts[3])
        case "threads" where count == 4:
            return .thread(teamSlug: team, threadId: parts[3])
        case "threads" where count == 3:
            return .threads(teamSlug: team)
        case "agents" where count == 4:
            return .agent(teamSlug: team, agentId: parts[3])
        case "agent" where count == 3:
            return .agentHome(teamSlug: team)
        case "inbox" where count == 3:
            return .inbox(teamSlug: team)
        case "devices" where count == 3:
            return .devices(teamSlug: team)
        case "settings" where count == 4 && parts[3] == "devices":
            return .devices(teamSlug: team)
        case "settings" where count == 4:
            return .settings(teamSlug: team, section: parts[3])
        case "settings" where count == 3:
            return .settings(teamSlug: team, section: "")
        case "apps" where count == 5:
            return .appScreen(teamSlug: team, appSlug: parts[3], screen: parts[4], params: screenQueryParams(written))
        case "apps" where count == 4:
            return .appHome(teamSlug: team, appSlug: parts[3])
        default:
            return .app(path: written)
        }
    }

    /// VAPP-218: the editor's own query keys, never a screen param
    /// (`Apps.ScreenSpec.reserved` reads this; kept here so AppLink compiles
    /// without `Apps` in the Share Extension).
    public static let reservedScreenKeys = ["tab", "frame"]

    /// VAPP-218: a path's query as screen params (the TS `screenQueryParams`):
    /// from the first `?` to the next `#`, `k=v` pairs split on `&` then the
    /// first `=` (none = the value ''), each side percent-decoded (a bad
    /// escape kept as written, no '+' rewriting), empty keys and the
    /// editor's own (`reservedScreenKeys`) dropped, the last repeat
    /// winning. Split on the UTF-8 bytes: the separators are ASCII, so every
    /// cut falls on a scalar boundary.
    static func screenQueryParams(_ path: String) -> [String: String] {
        let bytes = Array(path.utf8)
        guard let question = bytes.firstIndex(of: UInt8(ascii: "?")) else { return [:] }
        let start = question + 1
        let end = bytes[start...].firstIndex(of: UInt8(ascii: "#")) ?? bytes.endIndex
        var found: [String: String] = [:]
        for pair in bytes[start..<end].split(separator: UInt8(ascii: "&"), omittingEmptySubsequences: false) {
            let equals = pair.firstIndex(of: UInt8(ascii: "="))
            let rawKey = equals.map { pair[pair.startIndex..<$0] } ?? pair
            let key = decoded(String(decoding: rawKey, as: UTF8.self))
            if key.isEmpty || reservedScreenKeys.contains(key) { continue }
            found[key] = equals.map { decoded(String(decoding: pair[pair.index(after: $0)...], as: UTF8.self)) } ?? ""
        }
        return found
    }

    /// `decodeURIComponent`, a bad escape kept as written.
    private static func decoded(_ text: String) -> String {
        text.removingPercentEncoding ?? text
    }
}

/// #EXP-1674: what opening a resolved in-app link does (`AppLink.linkOpenOutcome`).
public enum LinkOpenOutcome: String, Equatable, Sendable {
    case open, switchTeam, noAccess, browser, notFound
}
