package com.exponential.app.domain

// FEED-42: the ONE copy of the GitHub connection block (Settings › Repositories)
// and the Add-repository picker. Byte-identical with web
// `github-connect-copy.ts`, desktop `github_connect.rs` and iOS
// `GithubCopy.swift` (typographic ’ everywhere); `GithubCopyTest` locks the
// literals mirrored from web.
object GithubCopy {
    // --- A. Connection block ---
    const val SECTION_TITLE = "Repositories"
    const val ADD_REPOSITORY = "Add repository"
    const val INTRO =
        "Connect a GitHub account or organization first, then add its repositories to share them " +
            "with the team — everyone can code on a shared repo. Point a board at one to make it " +
            "the clone target for “Start coding”."
    const val STATUS_FAILED = "Couldn’t reach GitHub connect state."
    const val RETRY = "Retry"
    const val NOT_CONFIGURED = "GitHub isn’t configured on this server."
    const val NOT_INSTALLED = "No GitHub account connected"
    const val CONNECT_GITHUB = "Connect GitHub"
    const val INSTALL_ON_ACCOUNT = "Install on an account"
    const val MANAGE = "Manage"
    const val ACCOUNTS_HEADER = "GitHub accounts connected to this team"
    const val CONFIGURE = "Configure"
    const val INSTALLATION_CAPTION =
        "An installation is per GitHub account or organization. Repositories come from the accounts listed here."
    const val CONNECT_ANOTHER_ACCOUNT = "Connect another account"
    const val REFRESH_ACCESS = "Refresh access"
    const val RECONNECT = "Reconnect"
    const val DISCONNECT_ACCOUNT = "Disconnect account"
    const val NO_REPOSITORIES = "No repositories connected yet."
    const val DISCONNECT_TITLE = "Disconnect GitHub account"
    const val DISCONNECT = "Disconnect"
    const val CANCEL = "Cancel"
    const val UNLINK_A11Y = "Disconnect this GitHub account from the team"

    /** An installation's display name: its login, else lower-case `installation {id}`. */
    fun installationLabel(accountLogin: String?, installationId: Long): String =
        accountLogin?.takeIf { it.isNotEmpty() } ?: "installation $installationId"

    fun suspendedLine(labels: List<String>): String =
        "GitHub suspended the Exponential app for ${labels.joinToString(", ")}. Unsuspend it on GitHub."

    fun reauthLine(labels: List<String>): String =
        "Reconnect GitHub to refresh which repositories you can access from ${labels.joinToString(", ")}."

    fun staleLine(label: String): String =
        "No one’s GitHub connection covers $label anymore — reconnecting can’t refresh it."

    fun unlinkConfirm(label: String): String =
        "This disconnects $label from the team. Repositories connected through it must be removed first."

    fun staleConfirm(label: String): String =
        "This removes $label from the team. Nobody’s GitHub connection covers it, so no repositories are lost."

    // --- B. Add-repository picker ---
    const val PICKER_TITLE = "Add repository"
    const val PICKER_LOADING = "Loading your GitHub repositories…"
    const val PICKER_NOT_CONFIGURED =
        "GitHub isn’t configured on this server, so repositories can’t be connected."
    const val PICKER_NOT_INSTALLED =
        "Connect the Exponential GitHub App to pick a repository. You’ll come right back here."
    const val I_HAVE_CONNECTED = "I’ve connected"
    const val SUSPENDED_FALLBACK_ACCOUNT = "a connected account"
    const val RECONNECT_GITHUB = "Reconnect GitHub"
    const val SEARCH_PLACEHOLDER = "Search repositories…"
    const val NO_RESULTS = "No repositories found."
    const val NONE_GRANTED = "None of your connected GitHub accounts grants a repository yet."
    const val FOOTER =
        "Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh."
    const val CAP_NOTE = "Showing the first 500 repositories per account — use the field below for the rest."
    const val REFRESH = "Refresh"
    const val INSTALL_ON_ANOTHER_ACCOUNT = "Install on another account"
    const val LOOKUP_PLACEHOLDER = "owner/name"
    const val LOOKUP_A11Y = "Add repository by name"
    const val LOOK_UP = "Look up"
    const val ADD_FORBIDDEN =
        "GitHub says you don’t have access to this repository, or your connection is stale. " +
            "Reconnect GitHub and try again."

    fun pickerSuspended(accounts: List<String?>): String {
        val names = accounts.map { it?.takeIf { login -> login.isNotEmpty() } ?: SUSPENDED_FALLBACK_ACCOUNT }
        return "GitHub suspended the Exponential app for ${names.joinToString(", ")}. " +
            "Its repositories can’t be connected until you unsuspend it on GitHub."
    }

    /** The picker's re-auth banner: [empty] = nothing granted at all. */
    fun reauthBanner(empty: Boolean, logins: List<String>): String = if (empty) {
        "Reconnect GitHub to load the repositories you can access" +
            (if (logins.isEmpty()) "" else " from ${logins.joinToString(", ")}") + "."
    } else {
        "Reconnect GitHub" + (if (logins.isEmpty()) "" else " (${logins.joinToString(", ")})") +
            " to refresh. Repos created or shared with you since your last connect won’t appear until you do."
    }

    /**
     * The grant-model refusal of `repositories.add` (desktop `is_grant_forbidden`):
     * a 403 whose server message tells the user to reconnect GitHub. Rendered as
     * [ADD_FORBIDDEN] + a "Reconnect GitHub" pill, never as an upsell.
     */
    fun isGrantForbidden(code: String?, status: Int?, message: String?): Boolean =
        (code == "FORBIDDEN" || status == 403) &&
            message?.lowercase()?.contains("reconnect github") == true
}
