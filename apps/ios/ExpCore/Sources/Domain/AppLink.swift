import Foundation

/// EXP-1188 — what a tapped markdown link in agent prose (narration, Guide,
/// thread) opens. Locked ×4 against `packages/domain-contract/fixtures/app-link.json`
/// (web `lib/app-link.ts`, desktop `domain::app_link`, Android `domain/AppLink.kt`);
/// the fixture's description is the rule. Hand-parsed on purpose: `URL` and
/// `URLComponents` normalise, reject and percent-encode differently from the
/// other three clients, and the point is ONE answer.
public enum AppLink: Equatable, Sendable {
    case issue(teamSlug: String, boardSlug: String, identifier: String)
    case session(teamSlug: String, sessionId: String)
    /// Another page on this instance, the path as written (query + hash kept).
    case app(path: String)
    /// Open as given (another origin, `mailto:`).
    case external(url: String)
    /// Nothing to open: placeholder hosts, `javascript:`, `//x`, `#x`, words.
    case ignore

    public static func classify(_ href: String, origin: String) -> AppLink {
        let trimmed = href.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return .ignore }
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

    /// `path` = everything after the authority (may be empty or start at `?`/`#`).
    private static func inApp(path: String) -> AppLink {
        let written = path.hasPrefix("/") ? path : "/" + path
        let pathOnly = written.prefix(while: { $0 != "?" && $0 != "#" })
        let parts = pathOnly.split(separator: "/").map { segment in
            String(segment).removingPercentEncoding ?? String(segment)
        }
        if parts.count == 6, parts[0] == "t", parts[2] == "boards", parts[4] == "issues" {
            return .issue(teamSlug: parts[1], boardSlug: parts[3], identifier: parts[5])
        }
        if parts.count == 4, parts[0] == "t", parts[2] == "sessions" {
            return .session(teamSlug: parts[1], sessionId: parts[3])
        }
        return .app(path: written)
    }
}
