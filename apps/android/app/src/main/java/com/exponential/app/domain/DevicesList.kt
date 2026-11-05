package com.exponential.app.domain

// A-16 / EXP-1560: the Devices page card — which group a device sits in,
// whether it says "No account", which logins wear the attention badge (and
// what their preview says), the update line its ⋯ menu leads with, and how
// many account icons show. A pure mirror of the TS
// reference `packages/domain-contract/src/devices-list.ts` (`deviceListRow`),
// byte-locked by `fixtures/devices-list.json` (`cases` + `accountIcons` +
// `accountPanelCases`, replayed by DevicesListTest). The fixture's `$comment` holds the rules.
//
// An AGENT with no login profile (a CLI installed, never signed in) adds
// nothing here: only reported login PROFILES are [DeviceListLogin]s.

object DevicesList {

    /** `devices-list.json` `copy`, byte-identical ×4. */
    object Copy {
        const val MINE = "My devices"
        const val TEAM = "Team devices"
        const val OFFLINE = "Offline"
        const val NO_ACCOUNT = "No account"
        const val CHECKING = "Checking…"
        const val ADD_ACCOUNT = "Add account"
        const val UPDATE_AVAILABLE = "Update available · v{version}"
        const val UPDATE_QUEUED = "Update queued: restarts within 10 minutes."
        const val NEEDS_RELOGIN = "{label} · Needs re-login"
        const val SIGNED_OUT = "{label} · Signed out"
        const val UPDATE = "Update"
        const val SIGN_IN = "Sign in"
        /** EXP-1627: the sign-in dialog's title and its ONE muted waiting line. */
        const val SIGN_IN_TITLE = "Sign in"
        const val SIGN_IN_WAITING = "Waiting for {device} to publish the {agent} sign-in link…"
        const val LIMIT_REACHED = "{label} · Limit reached"
        const val DEVICE_ACTIONS = "Device actions"
        const val SETTINGS = "Device settings"
        const val REMOVE_FROM_TEAM = "Remove from this team"
        const val MORE_ACCOUNTS = "More accounts"
        const val LONG_LIVED_REFUSED = "A long-lived login needs a Claude account on this machine."
        /** EXP-1626: the account menu's option on an existing signed-in claude login
         *  (and its renewal: the account menu only, no renewal reminder). */
        const val PERSIST_LOGIN = "Persist login"
        const val RENEW_PERSISTED_LOGIN = "Renew persisted login"
        /** EXP-1626: the persist sign-in dialog's one muted line. */
        const val PERSIST_NOTE =
            "Persist login keeps this account's runs signed in for 1 year, even if its browser login expires."
        /** EXP-1626: the preview header's line under the plan for a persisted login. */
        const val PERSISTED_BROWSER = "Persisted · usage from the browser login"
        const val PERSISTED_TOKEN = "Persisted · browser login expired, weekly usage only"

        /** `copy.status`: the account preview's warning-tone status line. */
        val STATUS: Map<Status, String> = mapOf(
            Status.NeedsRelogin to "Needs re-login",
            Status.SignedOut to "Signed out",
            Status.Limit to "Limit reached",
        )

        /** Fixture key → constant, for the replay test. */
        val all: Map<String, String> = mapOf(
            "mine" to MINE,
            "team" to TEAM,
            "offline" to OFFLINE,
            "noAccount" to NO_ACCOUNT,
            "checking" to CHECKING,
            "addAccount" to ADD_ACCOUNT,
            "updateAvailable" to UPDATE_AVAILABLE,
            "updateQueued" to UPDATE_QUEUED,
            "needsRelogin" to NEEDS_RELOGIN,
            "signedOut" to SIGNED_OUT,
            "update" to UPDATE,
            "signIn" to SIGN_IN,
            "signInTitle" to SIGN_IN_TITLE,
            "signInWaiting" to SIGN_IN_WAITING,
            "limitReached" to LIMIT_REACHED,
            "deviceActions" to DEVICE_ACTIONS,
            "settings" to SETTINGS,
            "removeFromTeam" to REMOVE_FROM_TEAM,
            "moreAccounts" to MORE_ACCOUNTS,
            "longLivedRefused" to LONG_LIVED_REFUSED,
            "persistLogin" to PERSIST_LOGIN,
            "renewPersistedLogin" to RENEW_PERSISTED_LOGIN,
            "persistNote" to PERSIST_NOTE,
            "persistedBrowser" to PERSISTED_BROWSER,
            "persistedToken" to PERSISTED_TOKEN,
        )
    }

    enum class Group(val wire: String) { Online("online"), Offline("offline") }

    enum class Kind(val wire: String) { Login("login"), Update("update"), UpdateQueued("update_queued") }

    enum class Action(val wire: String) { SignIn("sign_in"), Update("update") }

    /** EXP-1560: a login item's preview status (`copy.status` key). */
    enum class Status(val wire: String) {
        NeedsRelogin("needs_relogin"),
        SignedOut("signed_out"),
        Limit("limit"),
    }

    /** The agent whose logins can be persisted (`claude setup-token`). */
    const val KEEP_SIGNED_IN_AGENT = "claude"

    /** EXP-1560: the card grid's geometry (`fixture.card`), ×4. */
    object Card {
        const val MIN_WIDTH = 320
        const val GAP = 12
        const val PADDING = 12
        const val ICON_SIZE = 28
        const val MARK_SIZE = 16
        const val MAX_ACCOUNT_ICONS = 5
        /** #EXP-1643: the device's own icon leading the card. */
        const val DEVICE_ICON_SIZE = 20
        /** #EXP-1643: above and below the account panel's hairline. */
        const val PANEL_GAP = 12
    }

    /** EXP-1560: grid columns for a section [width] dp wide. */
    fun columns(width: Int): Int = maxOf(1, (width + Card.GAP) / (Card.MIN_WIDTH + Card.GAP))

    data class AccountIcons(val visible: Int, val more: Boolean)

    /** EXP-1560: how many account icons a card shows, and whether the ⋯
     *  "More accounts" button follows them (`fixture.accountIcons`). */
    fun accountIcons(count: Int, expanded: Boolean): AccountIcons =
        if (expanded || count <= Card.MAX_ACCOUNT_ICONS) {
            AccountIcons(visible = count, more = false)
        } else {
            AccountIcons(visible = Card.MAX_ACCOUNT_ICONS - 1, more = true)
        }

    /** #EXP-1643: what this client remembers of a card's open account. */
    sealed interface RememberedAccount {
        /** This device was never touched here. */
        data object Untouched : RememberedAccount

        /** Closed by hand. */
        data object Closed : RememberedAccount

        /** The login opened last. */
        data class Login(val key: String) : RememberedAccount
    }

    /** #EXP-1643: the login a card's panel shows (`fixture.accountPanelCases`). */
    fun openDeviceAccount(loginKeys: List<String>, remembered: RememberedAccount): String? = when {
        remembered == RememberedAccount.Closed -> null
        remembered is RememberedAccount.Login && remembered.key in loginKeys -> remembered.key
        else -> loginKeys.singleOrNull()
    }

    /**
     * EXP-1560: `limitHit` — a FRESH usage window at ≥ 100% (fresh = not
     * flagged stale and fetched within [AgentUsagePresentation.FRESH_WINDOW_MS]).
     */
    fun limitHit(usage: com.exponential.app.data.api.AgentUsage?, nowMs: Long): Boolean {
        if (usage == null || usage.stale) return false
        if (!AgentUsagePresentation.isFresh(usage.fetchedAt, nowMs)) return false
        return usage.windows.any { it.percent >= 100.0 }
    }

    data class DeviceListLogin(
        val agent: String,
        val profileId: String,
        /** [AgentAccountsRows.loginLabel]: the email, else the plan, else the fallback. */
        val label: String,
        val health: AgentHealth,
        /** EXP-1560: a fresh usage window at ≥ 100% ([limitHit]). */
        val limitHit: Boolean = false,
        /** EXP-1600: when the profile's long-lived claude login ends (ISO); null = none. */
        val tokenExpiresAt: String? = null,
    )

    data class Input(
        val online: Boolean,
        /** The caller's own machine (not a teammate's shared server). */
        val mine: Boolean,
        /** A headless CLI server: the only kind that acts on an update request
         * (a desktop app updates itself). */
        val server: Boolean,
        val registered: Boolean,
        /** The client's own version compare said the device is behind [latest]. */
        val outdated: Boolean,
        val latest: String?,
        val updateQueued: Boolean,
        /** The machine sent its accounts at least once (`agentAccounts` present). */
        val accountsReported: Boolean,
        /** Every login PROFILE it reported; an agent with none adds nothing. */
        val logins: List<DeviceListLogin>,
        /** EXP-1600: the clock the renewal reminder reads (ISO). */
        val now: String,
    )

    data class AttentionItem(
        val kind: Kind,
        val text: String,
        val action: Action?,
        /** Set on [Kind.Login] only. */
        val agent: String? = null,
        val profileId: String? = null,
        /** EXP-1560: the preview's status line; set on [Kind.Login] only. */
        val status: Status? = null,
    )

    data class Row(
        val group: Group,
        val noAccount: Boolean,
        val attention: List<AttentionItem>,
    )

    fun row(input: Input): Row {
        val attention = mutableListOf<AttentionItem>()
        val canSignIn = input.mine && input.online
        for ((health, template, status) in listOf(
            Triple(AgentHealth.NeedsRelogin, Copy.NEEDS_RELOGIN, Status.NeedsRelogin),
            Triple(AgentHealth.SignedOut, Copy.SIGNED_OUT, Status.SignedOut),
        )) {
            for (login in input.logins) {
                if (login.health != health) continue
                attention += AttentionItem(
                    kind = Kind.Login,
                    text = template.replace("{label}", login.label),
                    action = if (canSignIn) Action.SignIn else null,
                    agent = login.agent,
                    profileId = login.profileId,
                    status = status,
                )
            }
        }
        fun dead(login: DeviceListLogin) =
            login.health == AgentHealth.NeedsRelogin || login.health == AgentHealth.SignedOut
        for (login in input.logins) {
            if (!login.limitHit || dead(login)) continue
            attention += AttentionItem(
                kind = Kind.Login,
                text = Copy.LIMIT_REACHED.replace("{label}", login.label),
                action = null,
                agent = login.agent,
                profileId = login.profileId,
                status = Status.Limit,
            )
        }
        // EXP-1626: a persisted login is never an item (no renewal reminder).
        if (input.mine) {
            if (input.updateQueued) {
                attention += AttentionItem(Kind.UpdateQueued, Copy.UPDATE_QUEUED, null)
            } else if (input.outdated && input.latest != null) {
                attention += AttentionItem(
                    kind = Kind.Update,
                    text = Copy.UPDATE_AVAILABLE.replace("{version}", input.latest),
                    action = if (input.server && input.online && input.registered) Action.Update else null,
                )
            }
        }
        return Row(
            group = if (input.online) Group.Online else Group.Offline,
            noAccount = input.accountsReported && input.logins.isEmpty(),
            attention = attention,
        )
    }

    /** The pill label an item's action wears. */
    fun actionLabel(item: AttentionItem): String? = when (item.action) {
        null -> null
        Action.SignIn -> Copy.SIGN_IN
        Action.Update -> Copy.UPDATE
    }

    /**
     * Stable partition: online rows first in the list's own order, then the
     * offline ones (which render under the muted [Copy.OFFLINE] label).
     */
    fun <T> grouped(rows: List<T>, online: (T) -> Boolean): Pair<List<T>, List<T>> =
        rows.partition(online)
}
