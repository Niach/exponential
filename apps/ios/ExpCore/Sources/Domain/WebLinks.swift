import Foundation

/// Builds shareable web URLs into the running instance, mirroring the web app's
/// route shape:
///   issue:  {base}/w/{workspaceSlug}/projects/{projectSlug}/issues/{identifier}
///
/// The web is renaming workspaces → teams and moving `/w/…` to `/t/…` (EXP-122).
/// Construction still emits `/w/` for now — old links live in the wild forever
/// and the web redirects `/w/` → `/t/`, so there's no rush to flip emission —
/// while PARSING accepts both `/w/` and `/t/` so freshly-minted `/t/` universal
/// links open in the app too.
///
/// `base` is the account's `instanceUrl` (per `AuthRepository`); any trailing
/// slash is trimmed first (precedent: WorkspaceRepositoriesSection.webSettingsURL).
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

    /// `{base}/w/{workspaceSlug}/projects/{projectSlug}/issues/{identifier}`
    public static func issue(
        instanceUrl: String?, workspaceSlug: String, projectSlug: String, identifier: String
    ) -> URL? {
        guard let base = normalizedBase(instanceUrl) else { return nil }
        let ws = encode(workspaceSlug)
        let proj = encode(projectSlug)
        let id = encode(identifier)
        return URL(string: "\(base)/w/\(ws)/projects/\(proj)/issues/\(id)")
    }

    private static func encode(_ segment: String) -> String {
        segment.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? segment
    }

    /// A web-app URL the native app can render (EXP-92 Universal Links) — the
    /// inverse of `issue(...)` plus the invite route. Only these two shapes are
    /// claimed in the associated-domains AASA; anything else returns nil.
    public enum Parsed: Equatable, Sendable {
        case issue(workspaceSlug: String, projectSlug: String, identifier: String)
        case invite(token: String)
    }

    /// Parse `{base}/w/{ws}/projects/{proj}/issues/{identifier}` (and its `/t/`
    /// twin, EXP-122) plus `{base}/invite/{token}`. Splitting `url.path`
    /// (already percent-decoded) on "/" drops empty segments, so a trailing
    /// slash is tolerated while deeper paths (e.g. an issue's sub-tab) fail the
    /// exact-length match — deliberate: the app should only claim what it can
    /// render. Both `/w/` (legacy, permanent) and `/t/` (post-rename) resolve.
    public static func parse(_ url: URL) -> Parsed? {
        let parts = url.path.split(separator: "/").map(String.init)
        if parts.count == 6, parts[0] == "w" || parts[0] == "t",
           parts[2] == "projects", parts[4] == "issues" {
            return .issue(workspaceSlug: parts[1], projectSlug: parts[3], identifier: parts[5])
        }
        if parts.count == 2, parts[0] == "invite" {
            return .invite(token: parts[1])
        }
        return nil
    }
}
