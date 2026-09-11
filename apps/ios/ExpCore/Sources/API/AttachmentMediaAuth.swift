import Foundation

/// EXP-824 — the request headers an `AVURLAsset` needs to stream an
/// attachment's bytes. iOS is bearer-only (no cookies), and the byte route is
/// member-only, so the player has to carry the account's token itself — but
/// ONLY when the resolved URL sits on the account's own instance. A markdown
/// link pointing at any other host is streamed anonymously, or the session
/// token would leak to that host (the `AttachmentURL.isSameOrigin` rule the
/// image loader already enforces).
///
/// Foundation-only so the rule is unit-tested here; the AVFoundation call
/// site in the app target just passes the result as
/// `AVURLAssetHTTPHeaderFieldsKey`.
public enum AttachmentMediaAuth {
    /// Resolve `urlString` (stored relative `/api/attachments/{id}` form or
    /// absolute) against the instance and return the headers to attach: the
    /// bearer + the client-version gate header on the instance's own origin,
    /// nothing at all elsewhere. Nil when the URL cannot be resolved.
    public static func request(
        for urlString: String,
        instanceBaseURL: URL?,
        token: String?
    ) -> (url: URL, headers: [String: String])? {
        guard let resolved = AttachmentsApi.resolve(urlString, baseURL: instanceBaseURL) else {
            return nil
        }
        return (resolved, headers(for: resolved, instanceBaseURL: instanceBaseURL, token: token))
    }

    public static func headers(
        for url: URL,
        instanceBaseURL: URL?,
        token: String?
    ) -> [String: String] {
        guard let token, !token.isEmpty,
              AttachmentsApi.isSameOrigin(url, as: instanceBaseURL) else { return [:] }
        return [
            "Authorization": "Bearer \(token)",
            "x-client-version": AppConstants.clientVersionHeaderValue,
        ]
    }
}
