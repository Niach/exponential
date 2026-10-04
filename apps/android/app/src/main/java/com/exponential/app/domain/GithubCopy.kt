package com.exponential.app.domain

// SLOP-7/SLOP-26: the ONE copy of the GitHub connection block (Settings ›
// Repositories) and the Add-repository picker. Byte-identical with web
// `github-connect-copy.ts` (its settings + picker halves), desktop
// `github_connect.rs` and iOS `GithubCopy.swift` (typographic ’ everywhere);
// `GithubCopyTest` locks the literals mirrored from web. ONE vocabulary: a
// person CONNECTS their GitHub account, INSTALLS the Exponential app on the
// accounts whose repositories they want, and ADDS repositories they can push to.
object GithubCopy {
    // --- A. Connection block (Settings › Repositories) ---
    const val SECTION_TITLE = "Repositories"
    const val ADD_REPOSITORY = "Add repository"
    const val STATUS_FAILED = "Couldn’t reach GitHub connect state."
    const val RETRY = "Retry"
    const val NOT_CONFIGURED = "GitHub isn’t configured on this server."
    const val NOT_LINKED = "No GitHub account connected"
    const val CONNECT_GITHUB = "Connect GitHub"
    const val CONNECTED = "GitHub connected"
    const val RECONNECT_NEEDED = "Your GitHub connection expired."
    const val RECONNECT = "Reconnect"
    const val RECONNECT_GITHUB = "Reconnect GitHub"
    const val DISCONNECT = "Disconnect"
    const val DISCONNECT_TITLE = "Disconnect GitHub"
    const val DISCONNECT_BODY =
        "This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect."
    const val CANCEL = "Cancel"
    const val NOT_INSTALLED = "The Exponential app isn’t installed on any of your GitHub accounts yet."
    const val INSTALL_APP = "Install the app"
    const val INSTALL_ANOTHER = "Install on another account"
    const val ACCOUNTS_HEADER = "GitHub accounts with the app installed"
    const val CONFIGURE = "Configure"
    const val INSTALLATION_CAPTION = "Repositories come from these accounts. Configure one to grant more."
    const val MANAGE = "Manage"
    const val NO_REPOSITORIES = "No repositories added yet."
    /** The guided page's failed-link line, shown when a github-connected return carries an error. */
    const val LINK_FAILED = "GitHub didn’t finish connecting. Try again."

    /** An installation's display name: its login, else lower-case `installation {id}`. */
    fun installationLabel(accountLogin: String?, installationId: Long): String =
        accountLogin?.takeIf { it.isNotEmpty() } ?: "installation $installationId"

    fun connectedAs(login: String): String = "Connected as $login"

    fun configureTitle(label: String): String = "Configure which repositories $label grants on GitHub"

    fun suspendedLine(labels: List<String>): String =
        "GitHub suspended the Exponential app for ${labels.joinToString(", ")}. Unsuspend it on GitHub."

    // --- B. Add-repository picker ---
    const val PICKER_TITLE = "Add repository"
    const val PICKER_LOADING = "Loading your GitHub repositories…"
    const val PICKER_NOT_CONFIGURED =
        "GitHub isn’t configured on this server, so repositories can’t be added."
    const val PICKER_NOT_LINKED =
        "Connect your GitHub account to pick a repository. You’ll come right back here."
    const val PICKER_NOT_INSTALLED =
        "Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here."
    const val PICKER_CONNECTED_CHECK = "I’ve done that"
    const val SUSPENDED_FALLBACK_ACCOUNT = "a connected account"
    const val PICKER_RECONNECT_BANNER = "Your GitHub connection expired. Reconnect to list your repositories."
    const val SEARCH_PLACEHOLDER = "Search repositories…"
    const val NO_RESULTS = "No repositories found."
    const val NONE_PUSHABLE =
        "The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh."
    const val FOOTER =
        "Only repositories you can push to, on accounts where the app is installed, appear here. " +
            "Missing one? Grant it on GitHub, then refresh."
    const val CAP_NOTE = "Showing the first 500 repositories per account — use the field below for the rest."
    const val REFRESH = "Refresh"
    const val LOOKUP_PLACEHOLDER = "owner/name"
    const val LOOKUP_A11Y = "Add repository by name"
    const val LOOK_UP = "Look up"
    const val ADD_FORBIDDEN =
        "GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again."

    fun pickerSuspended(accounts: List<String?>): String {
        val names = accounts.map { it?.takeIf { login -> login.isNotEmpty() } ?: SUSPENDED_FALLBACK_ACCOUNT }
        return "GitHub suspended the Exponential app for ${names.joinToString(", ")}. " +
            "Its repositories can’t be added until you unsuspend it on GitHub."
    }

    /**
     * The FORBIDDEN arm of a failed `repositories.add` (desktop
     * `is_grant_forbidden`, web `connectForbidden`): a 403 whose server message
     * tells the user to reconnect GitHub. Rendered as [ADD_FORBIDDEN] + a
     * "Reconnect GitHub" pill, never as an upsell.
     */
    fun isGrantForbidden(code: String?, status: Int?, message: String?): Boolean =
        (code == "FORBIDDEN" || status == 403) &&
            message?.lowercase()?.contains("reconnect github") == true
}
