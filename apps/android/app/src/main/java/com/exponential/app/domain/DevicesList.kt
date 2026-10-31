package com.exponential.app.domain

// A-16: the Devices page row — which group a device sits in, whether it says
// "No account", and what its attention card lists. A pure mirror of the TS
// reference `packages/domain-contract/src/devices-list.ts` (`deviceListRow`),
// byte-locked by `fixtures/devices-list.json` (`cases`, replayed by
// DevicesListTest). The fixture's `$comment` holds the rules.
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
        const val ATTENTION = "Needs attention"
        const val UPDATE_AVAILABLE = "Update available · v{version}"
        const val UPDATE_QUEUED = "Update queued: restarts within 10 minutes."
        const val NEEDS_RELOGIN = "{label} · Needs re-login"
        const val SIGNED_OUT = "{label} · Signed out"
        const val UPDATE = "Update"
        const val SIGN_IN = "Sign in"

        /** Fixture key → constant, for the replay test. */
        val all: Map<String, String> = mapOf(
            "mine" to MINE,
            "team" to TEAM,
            "offline" to OFFLINE,
            "noAccount" to NO_ACCOUNT,
            "checking" to CHECKING,
            "addAccount" to ADD_ACCOUNT,
            "attention" to ATTENTION,
            "updateAvailable" to UPDATE_AVAILABLE,
            "updateQueued" to UPDATE_QUEUED,
            "needsRelogin" to NEEDS_RELOGIN,
            "signedOut" to SIGNED_OUT,
            "update" to UPDATE,
            "signIn" to SIGN_IN,
        )
    }

    enum class Group(val wire: String) { Online("online"), Offline("offline") }

    enum class Kind(val wire: String) { Login("login"), Update("update"), UpdateQueued("update_queued") }

    enum class Action(val wire: String) { SignIn("sign_in"), Update("update") }

    data class DeviceListLogin(
        val agent: String,
        val profileId: String,
        /** [AgentAccountsRows.loginLabel]: the email, else the plan, else the fallback. */
        val label: String,
        val health: AgentHealth,
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
    )

    data class AttentionItem(
        val kind: Kind,
        val text: String,
        val action: Action?,
        /** Set on [Kind.Login] only. */
        val agent: String? = null,
        val profileId: String? = null,
    )

    data class Row(
        val group: Group,
        val noAccount: Boolean,
        val attention: List<AttentionItem>,
    )

    fun row(input: Input): Row {
        val attention = mutableListOf<AttentionItem>()
        val canSignIn = input.mine && input.online
        for ((health, template) in listOf(
            AgentHealth.NeedsRelogin to Copy.NEEDS_RELOGIN,
            AgentHealth.SignedOut to Copy.SIGNED_OUT,
        )) {
            for (login in input.logins) {
                if (login.health != health) continue
                attention += AttentionItem(
                    kind = Kind.Login,
                    text = template.replace("{label}", login.label),
                    action = if (canSignIn) Action.SignIn else null,
                    agent = login.agent,
                    profileId = login.profileId,
                )
            }
        }
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
    fun actionLabel(action: Action): String = when (action) {
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
