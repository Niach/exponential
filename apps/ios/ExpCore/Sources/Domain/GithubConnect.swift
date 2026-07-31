import Foundation

/// EXP-390: the `exponential://github-connected[?error=<code>]` deep link's
/// error leg. The server's browser return page hands every connect outcome
/// back to the app through that link; dropping the `error` query (as the app
/// did before) made every failed connect look like a silent no-op — the
/// browser sheet closed and nothing happened. Copy mirrors the desktop's
/// `connect_error_message` (github_connect.rs) — keep the three natives'
/// strings identical (not a byte-parity contract, just courtesy).
public enum GithubConnect {
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

    /// Human copy per server error code; unknown codes get the generic line.
    public static func errorMessage(for slug: String) -> String {
        switch slug {
        case "session":
            return "The connect link expired or was already used. Try connecting again."
        case "exchange":
            return "GitHub sign-in didn't complete. Try connecting again."
        case "none":
            return "The Exponential GitHub App isn't installed for any account you can access yet. Use Connect GitHub to install it."
        case "notowner":
            return "The authorized GitHub account only has collaborator access to existing installations. Install the App on your own account or organization."
        case "orgperm":
            return "Your organization hasn't approved the App's members-read permission yet. An org admin must accept it on GitHub, then reconnect."
        case "forbidden":
            return "Only team owners can connect GitHub accounts to a team."
        default:
            return "Something went wrong while connecting GitHub. Please try again."
        }
    }
}
