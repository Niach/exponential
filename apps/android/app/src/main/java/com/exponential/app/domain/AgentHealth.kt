package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile

// EXP-849: account HEALTH — how USABLE a login is, as the machine's usage
// probe saw it.
//
// `auth status` answers WHO a login is (email, plan) and nothing about whether
// it still works; the device's usage probe answers that (an Unauthorized probe
// is a dead credential, a successful one a live account) and writes it onto the
// account and every `profiles[]` entry as `health`. Everything here is the
// PRESENTATION of that field, hand-mirrored with web `lib/agent-usage.ts`
// (`agentHealth` / `healthBadgeLabel` / `healthRank` / `worstHealth` /
// `deviceWorstHealth`), the desktop's `coding::agent_accounts::Health` and the
// iOS account rows: same four values, same fallback, same two badge strings.

/** The four wire values, byte-identical ×4 (db-schema `deviceAgentHealthValues`). */
enum class AgentHealth(val wire: String) {
    Ok("ok"),
    NeedsRelogin("needs_relogin"),
    SignedOut("signed_out"),
    Unknown("unknown"),
}

object AgentHealthRules {

    /**
     * A wire token back into the enum; anything else (a newer device's value,
     * junk) is [AgentHealth.Unknown] — a health name this build does not know
     * must degrade to "nothing claimed", never blank the row.
     */
    fun parse(raw: String?): AgentHealth {
        val token = raw?.trim()?.takeIf { it.isNotEmpty() } ?: return AgentHealth.Unknown
        return AgentHealth.entries.firstOrNull { it.wire == token } ?: AgentHealth.Unknown
    }

    /**
     * What a row with no `health` at all means (a pre-EXP-849 device): the only
     * thing its payload says is whether the CLI was signed in.
     */
    fun derived(signedIn: Boolean): AgentHealth =
        if (signedIn) AgentHealth.Ok else AgentHealth.SignedOut

    /**
     * One account's health: the device's own verdict when it sent one, else
     * derived from `signedIn`. A `signed_out` claim from a signed-IN report is
     * kept — the device is the authority on its own credential.
     */
    fun of(account: AgentAccount?): AgentHealth {
        if (account == null) return AgentHealth.Unknown
        val reported = account.health?.trim()?.takeIf { it.isNotEmpty() }
        return if (reported != null) parse(reported) else derived(account.signedIn)
    }

    /** One profile's health — same vocabulary, same fallback. */
    fun of(profile: AgentAccountProfile): AgentHealth {
        val reported = profile.health?.trim()?.takeIf { it.isNotEmpty() }
        return if (reported != null) parse(reported) else derived(profile.signedIn)
    }

    /**
     * The badge a row carries, or null when there is nothing to say (`ok`, and
     * `unknown` — "signed in, never probed" is not a problem). The two
     * negatives are DISTINCT on purpose: "Signed out" is a login you never
     * made, "Needs re-login" one that expired under you.
     */
    fun badgeLabel(health: AgentHealth): String? = when (health) {
        AgentHealth.NeedsRelogin -> "Needs re-login"
        AgentHealth.SignedOut -> "Signed out"
        else -> null
    }

    /**
     * Worst-FIRST ordering of the four values (web `healthRank`): an expired
     * credential is the one thing a human has to act on, a missing login next,
     * then a login nobody has probed, then a working one.
     */
    fun rank(health: AgentHealth): Int = when (health) {
        AgentHealth.NeedsRelogin -> 0
        AgentHealth.SignedOut -> 1
        AgentHealth.Unknown -> 2
        AgentHealth.Ok -> 3
    }

    /**
     * The worst health in a set — what a DEVICE row badges (its accounts'
     * worst) and what an account row shows across its machines. Null for an
     * empty set: nothing was reported, so nothing is claimed.
     */
    fun worst(healths: Iterable<AgentHealth>): AgentHealth? =
        healths.minByOrNull { rank(it) }

    /**
     * The health a DEVICE row badges — the worst among every account it
     * reported (each agent's profiles, or the top-level account for a
     * pre-profile machine). Null when the machine reported no account at all.
     */
    fun deviceWorst(accounts: Map<String, AgentAccount>?): AgentHealth? {
        if (accounts.isNullOrEmpty()) return null
        val healths = mutableListOf<AgentHealth>()
        for (account in accounts.values) {
            val profiles = account.profiles.orEmpty()
            if (profiles.isEmpty()) {
                healths += of(account)
            } else {
                profiles.forEach { healths += of(it) }
            }
        }
        return worst(healths)
    }
}
