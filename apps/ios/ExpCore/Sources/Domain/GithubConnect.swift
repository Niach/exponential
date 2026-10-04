import Foundation

/// SLOP-26: the pure half of the ONE GitHub flow on the phone — which
/// prerequisite is missing (the picker and the connection block name it with
/// its one fix), and the `exponential://github-connected` return's shape.
/// Android's `domain/GithubConnect.kt` writes the same rules.
public enum GithubConnect {
    /// What stands between the viewer and the live repository list, in the
    /// order the guided web page walks them.
    public enum Prerequisite: Equatable, Sendable {
        /// GitHub isn't configured on this server — nothing to do here.
        case notConfigured
        /// No GitHub account linked → Connect GitHub.
        case notLinked
        /// Linked, token dead → Reconnect GitHub.
        case expired
        /// Linked, the App installed nowhere the token sees → Install the app.
        case notInstalled
    }

    /// The first missing prerequisite, nil once repositories can be listed.
    public static func prerequisite(
        configured: Bool, linked: Bool, needsReconnect: Bool, installed: Bool
    ) -> Prerequisite? {
        if !configured { return .notConfigured }
        if !linked { return .notLinked }
        if needsReconnect { return .expired }
        if !installed { return .notInstalled }
        return nil
    }

    /// The `error` slug of a github-connected URL, `nil` when absent or empty
    /// (an error-less link is the success form). Works on both the deep link
    /// and the ASWebAuthenticationSession callback URL — same URL.
    public static func errorSlug(from url: URL) -> String? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            return nil
        }
        let slug = components.queryItems?.first { $0.name == "error" }?.value
        return (slug?.isEmpty ?? true) ? nil : slug
    }

    /// Whether a refused `users.mintSignInLinkTicket({provider: "github"})`
    /// means the server does not offer the GitHub link (BAD_REQUEST: "That
    /// sign-in provider is not offered on this instance") — the hop then falls
    /// back to the guided web page (`connectUrl`). Any other refusal is shown.
    public static func linkNotOffered(code: String?) -> Bool {
        code == "BAD_REQUEST"
    }
}
