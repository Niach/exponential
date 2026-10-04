import Foundation

/// SLOP-7/SLOP-26: the ONE copy of the GitHub connection block (Settings ›
/// Repositories) and the Add-repository picker. Byte-identical to web
/// `github-connect-copy.ts` (its settings + picker halves), desktop
/// `github_connect.rs` and Android `domain/GithubCopy.kt` (typographic ’
/// everywhere) — `GithubCopyTests` locks the literals. ONE vocabulary: a
/// person CONNECTS their GitHub account, INSTALLS the Exponential app on the
/// accounts whose repositories they want, and ADDS repositories they can push
/// to.
public enum GithubCopy {
    // MARK: - Connection block (Settings › Repositories)

    public static let sectionTitle = "Repositories"
    public static let addRepository = "Add repository"
    public static let statusFailed = "Couldn’t reach GitHub connect state."
    public static let retry = "Retry"
    public static let notConfigured = "GitHub isn’t configured on this server."
    public static let notLinked = "No GitHub account connected"
    public static let connectGithub = "Connect GitHub"
    public static let connected = "GitHub connected"
    public static let reconnectNeeded = "Your GitHub connection expired."
    public static let reconnect = "Reconnect"
    public static let reconnectGithub = "Reconnect GitHub"
    public static let disconnect = "Disconnect"
    public static let disconnectTitle = "Disconnect GitHub"
    public static let disconnectBody =
        "This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect."
    public static let cancel = "Cancel"
    public static let notInstalled = "The Exponential app isn’t installed on any of your GitHub accounts yet."
    public static let installApp = "Install the app"
    public static let installAnother = "Install on another account"
    public static let accountsHeader = "GitHub accounts with the app installed"
    public static let configure = "Configure"
    public static let installationCaption = "Repositories come from these accounts. Configure one to grant more."
    public static let manage = "Manage"
    public static let noRepositories = "No repositories added yet."
    /// The guided page's failed-link line, shown when a github-connected
    /// return carries an error.
    public static let linkFailed = "GitHub didn’t finish connecting. Try again."

    /// The account label: the login, else lower-case `installation {id}`.
    public static func installationLabel(login: String?, installationId: Int) -> String {
        if let login, !login.isEmpty { return login }
        return "installation \(installationId)"
    }

    public static func connectedAs(_ login: String) -> String {
        "Connected as \(login)"
    }

    public static func configureTitle(_ label: String) -> String {
        "Configure which repositories \(label) grants on GitHub"
    }

    public static func suspendedLine(_ labels: [String]) -> String {
        "GitHub suspended the Exponential app for \(labels.joined(separator: ", ")). Unsuspend it on GitHub."
    }

    // MARK: - Add-repository picker

    public static let pickerTitle = "Add repository"
    public static let pickerLoading = "Loading your GitHub repositories…"
    public static let pickerNotConfigured =
        "GitHub isn’t configured on this server, so repositories can’t be added."
    public static let pickerNotLinked =
        "Connect your GitHub account to pick a repository. You’ll come right back here."
    public static let pickerNotInstalled =
        "Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here."
    public static let pickerConnectedCheck = "I’ve done that"
    public static let suspendedFallback = "a connected account"
    public static let pickerReconnectBanner = "Your GitHub connection expired. Reconnect to list your repositories."
    public static let searchPlaceholder = "Search repositories…"
    public static let noMatch = "No repositories found."
    public static let nonePushable =
        "The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh."
    public static let footerExplain =
        "Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh."
    public static let capNote =
        "Showing the first 500 repositories per account — use the field below for the rest."
    public static let refresh = "Refresh"
    public static let lookupPlaceholder = "owner/name"
    public static let lookupAccessibility = "Add repository by name"
    public static let lookUp = "Look up"
    public static let addForbidden =
        "GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again."

    /// The picker's suspended banner; a login-less account reads
    /// "a connected account".
    public static func pickerSuspended(_ logins: [String?]) -> String {
        let names = logins.map { login -> String in
            if let login, !login.isEmpty { return login }
            return suspendedFallback
        }.joined(separator: ", ")
        return "GitHub suspended the Exponential app for \(names). Its repositories can’t be added until you unsuspend it on GitHub."
    }

    /// The FORBIDDEN arm of a failed add (desktop `is_grant_forbidden`
    /// parity, web `connectForbidden`): a FORBIDDEN whose server message
    /// points at a GitHub reconnect — never misread as a plan limit.
    public static func isGrantForbidden(code: String?, message: String) -> Bool {
        code == "FORBIDDEN" && message.lowercased().contains("reconnect github")
    }
}
