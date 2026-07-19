import Foundation

/// Builds shareable web URLs into the running instance, mirroring the web app's
/// route shape:
///   issue:  {base}/t/{teamSlug}/boards/{boardSlug}/issues/{identifier}
///
/// EXP-180 (the great rename): the web's canonical routes are `/t/…/boards/…`
/// and the legacy `/w/` + `/projects/` forms are DEAD server-side (no
/// redirects), so this builder emits ONLY the new form and the parser resolves
/// ONLY the new form — a dead route must not deep-link into the app either.
///
/// `base` is the account's `instanceUrl` (per `AuthRepository`); any trailing
/// slash is trimmed first (precedent: TeamRepositoriesSection.webSettingsURL).
/// Slug/identifier path segments are percent-encoded defensively even though the
/// server only ever mints URL-safe slugs. (Board-level links were removed —
/// sharing is issue-only on every client.)
public enum WebLinks {
    /// Normalize an instance base URL string into a trailing-slash-free base.
    /// Returns nil for an empty/whitespace-only input.
    public static func normalizedBase(_ instanceUrl: String?) -> String? {
        guard let raw = instanceUrl?.trimmingCharacters(in: .whitespacesAndNewlines),
              !raw.isEmpty else { return nil }
        return raw.hasSuffix("/") ? String(raw.dropLast()) : raw
    }

    /// `{base}/t/{teamSlug}/boards/{boardSlug}/issues/{identifier}`
    public static func issue(
        instanceUrl: String?, teamSlug: String, boardSlug: String, identifier: String
    ) -> URL? {
        guard let base = normalizedBase(instanceUrl) else { return nil }
        let team = encode(teamSlug)
        let board = encode(boardSlug)
        let id = encode(identifier)
        return URL(string: "\(base)/t/\(team)/boards/\(board)/issues/\(id)")
    }

    private static func encode(_ segment: String) -> String {
        segment.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? segment
    }

    /// A web-app URL the native app can render (EXP-92 Universal Links) — the
    /// inverse of `issue(...)` plus the invite route. Only these two shapes are
    /// claimed in the associated-domains AASA; anything else returns nil.
    public enum Parsed: Equatable, Sendable {
        case issue(teamSlug: String, boardSlug: String, identifier: String)
        case invite(token: String)
    }

    /// Parse `{base}/t/{team}/boards/{board}/issues/{identifier}` plus
    /// `{base}/invite/{token}`. Splitting `url.path` (already percent-decoded)
    /// on "/" drops empty segments, so a trailing slash is tolerated while
    /// deeper paths (e.g. an issue's sub-tab) fail the exact-length match —
    /// deliberate: the app should only claim what it can render. The legacy
    /// `/w/` + `/projects/` forms are dead on the web (no redirects), so they
    /// deliberately return nil here too.
    public static func parse(_ url: URL) -> Parsed? {
        let parts = url.path.split(separator: "/").map(String.init)
        if parts.count == 6, parts[0] == "t",
           parts[2] == "boards", parts[4] == "issues" {
            return .issue(teamSlug: parts[1], boardSlug: parts[3], identifier: parts[5])
        }
        if parts.count == 2, parts[0] == "invite" {
            return .invite(token: parts[1])
        }
        return nil
    }
}
