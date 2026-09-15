import Foundation

/// FEED-42: the ONE copy of the GitHub connection block (Settings ›
/// Repositories) and the Add-repository picker. Byte-identical to web
/// `github-connect-copy.ts`, desktop `github_connect.rs` and Android
/// `domain/GithubCopy.kt` (typographic ’ everywhere) — `GithubCopyTests`
/// locks the literals.
public enum GithubCopy {
    // MARK: - Connection block

    public static let sectionTitle = "Repositories"
    public static let addRepository = "Add repository"
    public static let intro =
        "Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”."
    public static let statusFailed = "Couldn’t reach GitHub connect state."
    public static let retry = "Retry"
    public static let notConfigured = "GitHub isn’t configured on this server."
    public static let notInstalled = "No GitHub account connected"
    public static let connectGithub = "Connect GitHub"
    public static let installOnAnAccount = "Install on an account"
    public static let manage = "Manage"
    public static let accountsHeader = "GitHub accounts connected to this team"
    public static let configure = "Configure"
    public static let disconnectAccessibility = "Disconnect this GitHub account from the team"
    public static let installationsCaption =
        "An installation is per GitHub account or organization. Repositories come from the accounts listed here."
    public static let connectAnotherAccount = "Connect another account"
    public static let refreshAccess = "Refresh access"
    public static let reconnect = "Reconnect"
    public static let disconnectAccount = "Disconnect account"
    public static let disconnectTitle = "Disconnect GitHub account"
    public static let cancel = "Cancel"
    public static let disconnect = "Disconnect"
    public static let noRepositories = "No repositories connected yet."

    /// The account label: the login, else lower-case `installation {id}`.
    public static func installationLabel(login: String?, installationId: Int) -> String {
        login ?? "installation \(installationId)"
    }

    public static func suspendedLine(_ labels: [String]) -> String {
        "GitHub suspended the Exponential app for \(labels.joined(separator: ", ")). Unsuspend it on GitHub."
    }

    public static func reauthLine(_ labels: [String]) -> String {
        "Reconnect GitHub to refresh which repositories you can access from \(labels.joined(separator: ", "))."
    }

    public static func staleLine(_ label: String) -> String {
        "No one’s GitHub connection covers \(label) anymore — reconnecting can’t refresh it."
    }

    /// Confirm copy for the ✕ on a LIVE link (the server refuses while a
    /// connected repo still rides it).
    public static func unlinkConfirm(_ label: String) -> String {
        "This disconnects \(label) from the team. Repositories connected through it must be removed first."
    }

    /// Confirm copy for a STALE link (zero grants from anyone).
    public static func staleConfirm(_ label: String) -> String {
        "This removes \(label) from the team. Nobody’s GitHub connection covers it, so no repositories are lost."
    }

    // MARK: - Add-repository picker

    public static let loadingRepos = "Loading your GitHub repositories…"
    public static let pickerNotConfigured =
        "GitHub isn’t configured on this server, so repositories can’t be connected."
    public static let pickerNotInstalled =
        "Connect the Exponential GitHub App to pick a repository. You’ll come right back here."
    public static let iveConnected = "I’ve connected"
    public static let suspendedFallback = "a connected account"
    public static let reconnectGithub = "Reconnect GitHub"
    public static let searchPlaceholder = "Search repositories…"
    public static let noneFound = "No repositories found."
    public static let noneGranted = "None of your connected GitHub accounts grants a repository yet."
    public static let footerSentence =
        "Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh."
    public static let capNote =
        "Showing the first 500 repositories per account — use the field below for the rest."
    public static let refresh = "Refresh"
    public static let installOnAnotherAccount = "Install on another account"
    public static let lookupPlaceholder = "owner/name"
    public static let lookupAccessibility = "Add repository by name"
    public static let lookUp = "Look up"
    public static let addForbidden =
        "GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again."

    /// The picker's suspended banner; a login-less account reads
    /// "a connected account".
    public static func pickerSuspended(_ logins: [String?]) -> String {
        let names = logins.map { $0 ?? suspendedFallback }.joined(separator: ", ")
        return "GitHub suspended the Exponential app for \(names). Its repositories can’t be connected until you unsuspend it on GitHub."
    }

    /// The picker's re-auth banner: the empty-list variant names accounts
    /// with " from a, b", the non-empty one with " (a, b)".
    public static func reauthBanner(accounts: [String], emptyList: Bool) -> String {
        if emptyList {
            let suffix = accounts.isEmpty ? "" : " from \(accounts.joined(separator: ", "))"
            return "Reconnect GitHub to load the repositories you can access\(suffix)."
        }
        let suffix = accounts.isEmpty ? "" : " (\(accounts.joined(separator: ", ")))"
        return "Reconnect GitHub\(suffix) to refresh. Repos created or shared with you since your last connect won’t appear until you do."
    }

    /// The grant-model FORBIDDEN arm of a failed add (desktop
    /// `is_grant_forbidden` parity): a FORBIDDEN whose server message points
    /// at a GitHub reconnect — never misread as a plan limit.
    public static func isGrantForbidden(code: String?, message: String) -> Bool {
        code == "FORBIDDEN" && message.lowercased().contains("reconnect github")
    }
}
