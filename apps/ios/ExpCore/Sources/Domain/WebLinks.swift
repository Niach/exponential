import Foundation

/// Builds shareable web URLs into the running instance, mirroring the web app's
/// route shape:
///   issue:  {base}/w/{workspaceSlug}/projects/{projectSlug}/issues/{identifier}
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
}
