package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.SYSTEM_PROFILE_ID

// EXP-849 phase 3: the MID-SESSION account switch, as the rule the session
// screen's account rows render.
//
// A switch is a RESUME that names an account (`steer.startSession({
// resumeSessionId, deviceId, account })`): the machine relaunches the run under
// the other login in the same worktree and links the new `coding_sessions` row
// by `resumed_from_id`, so the run reads as a continuation rather than a second
// run. Claude only — codex stays one account per session (EXP-849 §E) — and
// only between turns: a switch mid-turn would abandon work the agent has in
// flight.
//
// Everything here is pure. Hand-mirrored with the desktop's
// `ResumeRunRequest.account` gate and the web/iOS account rows: same refusals,
// same strings.

/** One login the live run could continue under. */
data class SessionAccountOption(
    /** What a start sends as `account` (`system` = the machine's ambient login). */
    val profileId: String,
    /** The chip label (`Default` for the ambient login when the device sent none). */
    val label: String,
    val email: String?,
    val plan: String?,
    val signedIn: Boolean,
    val health: AgentHealth,
    /** The machine's CURRENT login for the agent. */
    val active: Boolean,
    /**
     * EXP-909: the login THIS RUN is on — resolved once in
     * [SessionAccountSwitch.options] from the synced `agent_account`, else the
     * machine's reported email, else its active login. Exactly one option ever
     * carries it, and none does when the run's account is unknowable (which is
     * NOT the same as "the ambient login").
     */
    val current: Boolean,
    /** This login's own rate-limit windows, when the machine reported them. */
    val usage: AgentUsage?,
) {
    /** The identity line: the email, else the plan, else the profile label. */
    val caption: String get() = email ?: plan ?: label
}

object SessionAccountSwitch {

    /** The only agent that can change account between messages (EXP-849 §E). */
    const val SWITCHABLE_AGENT = "claude"

    /** The sheet's section title, byte-identical ×4. */
    const val SECTION_TITLE = "Accounts"

    /** The primary control on an account row. */
    const val SWITCH_LABEL = "Switch to this account"

    /** The rate-limit wall's PRIMARY button (EXP-849 interface D). */
    const val WALL_SWITCH_LABEL = "Switch account"

    /**
     * What a switch costs, said once where the switch is offered: the agent
     * re-enters the recorded run under the other login, which re-reads the
     * transcript — one extra context read, not a per-message surcharge.
     */
    const val COST_NOTE =
        "The run continues under the other account. Re-reading the transcript once costs tokens."

    // ── Refusals ────────────────────────────────────────────────────────────
    // One sentence each, and the reason is always about the thing the person
    // can change. Shown on the DISABLED control rather than hiding it, so the
    // switch never silently disappears mid-run.

    const val REASON_AGENT = "Only claude can switch accounts during a run."
    const val REASON_NOT_MINE = "Only the person who started this run can switch its account."
    const val REASON_ENDED = "This run has ended — resume it instead."
    const val REASON_OFFLINE = "The machine is offline."
    /**
     * The machine's build cannot take the switch: it either cannot resume a
     * run at all (`resume-run`) or does not honour `account` on a LIVE one
     * (`account-switch`, desktop 0.14.38). An older build would resume on the
     * RECORDED account and drop the field, so the "switch" would quietly keep
     * the exhausted login — the server refuses it, and so do these rows.
     */
    const val REASON_NO_CAP = "Update the app on that machine to switch accounts."
    const val REASON_BUSY = "The agent is working — switching waits for the turn to finish."
    const val REASON_SIGNED_OUT = "Sign in to this account on that machine first."
    const val REASON_NEEDS_RELOGIN = "This account needs a re-login on that machine."
    const val REASON_ALREADY = "This run is already on this account."

    /**
     * The logins the session screen lists for the run's [agent] on its host
     * machine: every profile the machine reported, or the single ambient
     * account for a pre-profile machine. Empty when the machine said nothing
     * about the agent — there is then nothing to switch between.
     *
     * EXP-909: [currentAccount] is the run's synced `coding_sessions.
     * agent_account`, and [activeAccountIndex] marks exactly one option (or
     * none) as the login the run is ON.
     */
    fun options(
        accounts: Map<String, AgentAccount>?,
        agent: String?,
        currentAccount: String? = null,
    ): List<SessionAccountOption> {
        val id = agent?.takeIf { it.isNotBlank() } ?: return emptyList()
        val account = accounts?.get(id) ?: return emptyList()
        val profiles = account.profiles.orEmpty()
        val listed = if (profiles.isEmpty()) {
            listOf(
                SessionAccountOption(
                    profileId = SYSTEM_PROFILE_ID,
                    label = "Default",
                    email = account.email?.trim()?.takeIf { it.isNotEmpty() },
                    plan = account.plan?.trim()?.takeIf { it.isNotEmpty() },
                    signedIn = account.signedIn,
                    health = AgentHealthRules.of(account),
                    active = true,
                    current = false,
                    usage = null,
                ),
            )
        } else {
            profiles.map { profile ->
                SessionAccountOption(
                    profileId = profile.id,
                    label = profile.label?.trim()?.takeIf { it.isNotEmpty() }
                        ?: if (profile.id == SYSTEM_PROFILE_ID) "Default" else profile.id,
                    email = profile.email?.trim()?.takeIf { it.isNotEmpty() },
                    plan = profile.plan?.trim()?.takeIf { it.isNotEmpty() },
                    signedIn = profile.signedIn,
                    health = AgentHealthRules.of(profile),
                    active = profile.active,
                    current = false,
                    usage = profile.usage,
                )
            }
        }
        val index = activeAccountIndex(
            listed,
            currentAccount,
            account.email?.trim()?.takeIf { it.isNotEmpty() },
        )
        if (index < 0) return listed
        return listed.mapIndexed { at, option -> option.copy(current = at == index) }
    }

    /**
     * EXP-909: WHICH listed login the run is on, in the only order a client
     * can know it (web `activeAccountIndex`, the desktop's `SwitchTarget.
     * current` rule):
     *  1. the synced `agent_account`, when it names a login this machine lists;
     *  2. else the login whose email matches the machine's top-level report for
     *     the agent (the device only ever puts its ACTIVE login's identity
     *     there);
     *  3. else the machine's active login.
     *
     * -1 = unknown, and unknown is NOT `system`: guessing the ambient login is
     * exactly the bug EXP-875 §1 was (a run on dennis@ headed danny@, with a
     * stale "Needs re-login" badge borrowed from the wrong account).
     */
    fun activeAccountIndex(
        options: List<SessionAccountOption>,
        currentAccount: String?,
        reportedEmail: String?,
    ): Int {
        val named = currentAccount?.trim()?.takeIf { it.isNotEmpty() }
        if (named != null) {
            val byId = options.indexOfFirst { it.profileId == named }
            if (byId >= 0) return byId
        }
        val email = reportedEmail?.trim()?.takeIf { it.isNotEmpty() }
        if (email != null) {
            val byEmail = options.indexOfFirst { it.email == email }
            if (byEmail >= 0) return byEmail
        }
        return options.indexOfFirst { it.active }
    }

    /**
     * Why [option] cannot be switched to right now, or null when it can.
     * Display gating only — the server and the machine re-check everything.
     *
     * [currentAccount] is the profile this run is KNOWN to be on. EXP-909 put
     * `coding_sessions.agent_account` on the shape, so it is usually the run's
     * own stamp; it stays nullable because a pre-EXP-909 row carries none, and
     * the machine's active login is not a safe stand-in for it.
     */
    fun refusal(
        option: SessionAccountOption,
        agent: String?,
        mine: Boolean,
        sessionEnded: Boolean,
        deviceOnline: Boolean,
        canResume: Boolean,
        canSwitchAccount: Boolean,
        turnState: String,
        currentAccount: String? = null,
    ): String? = when {
        agent != SWITCHABLE_AGENT -> REASON_AGENT
        !mine -> REASON_NOT_MINE
        sessionEnded -> REASON_ENDED
        !deviceOnline -> REASON_OFFLINE
        // BOTH caps: the switch is a resume (`resume-run`) that the machine
        // has to honour the `account` of on a live run (`account-switch`).
        !canResume || !canSwitchAccount -> REASON_NO_CAP
        // EXP-848's turn slot is the idle test: ENDED is the default, so a
        // viewer that has not seen a `turn` event yet reads as idle.
        turnState == TURN_STATE_STARTED -> REASON_BUSY
        option.health == AgentHealth.NeedsRelogin -> REASON_NEEDS_RELOGIN
        !option.signedIn || option.health == AgentHealth.SignedOut -> REASON_SIGNED_OUT
        currentAccount != null && currentAccount == option.profileId -> REASON_ALREADY
        else -> null
    }

    /**
     * What the switch carries as `account` — the picked profile VERBATIM,
     * `system` included.
     *
     * A fresh start omits the ambient login (`system` is the absence of an
     * account there), but a switch may not: the server reads the PRESENCE of
     * `account` as "this resume is a switch" and is the only thing that lets a
     * resume ride a LIVE run, so an omitted field would be refused with "That
     * run is still live — stop it first, or name an account to continue it on".
     * `system` is accepted there explicitly and skips the profile-membership
     * check (web `session-account-switch.tsx` sends the profile id verbatim
     * too).
     */
    fun wireAccount(option: SessionAccountOption): String = option.profileId
}
