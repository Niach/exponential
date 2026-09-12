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
        "Switching continues this run under the other account. The agent re-reads the " +
            "transcript once, which costs tokens."

    /** The continuation byline a resumed run's screen carries. */
    const val CONTINUATION_NOTE = "Continues an earlier run"

    /**
     * What that continuation cost, said ONCE on the new run: re-entering the
     * recorded run re-reads its transcript, which is a single extra context
     * read — not a per-message surcharge.
     */
    const val CONTINUATION_COST_NOTE =
        "The agent re-read the transcript once to pick it up — a one-time cost."

    // ── Refusals ────────────────────────────────────────────────────────────
    // One sentence each, and the reason is always about the thing the person
    // can change. Shown on the DISABLED control rather than hiding it, so the
    // switch never silently disappears mid-run.

    const val REASON_AGENT = "Only claude can switch accounts during a run."
    const val REASON_NOT_MINE = "Only the person who started this run can switch its account."
    const val REASON_ENDED = "This run has ended — resume it instead."
    const val REASON_OFFLINE = "The machine is offline."
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
     */
    fun options(accounts: Map<String, AgentAccount>?, agent: String?): List<SessionAccountOption> {
        val id = agent?.takeIf { it.isNotBlank() } ?: return emptyList()
        val account = accounts?.get(id) ?: return emptyList()
        val profiles = account.profiles.orEmpty()
        if (profiles.isEmpty()) {
            return listOf(
                SessionAccountOption(
                    profileId = SYSTEM_PROFILE_ID,
                    label = "Default",
                    email = account.email?.trim()?.takeIf { it.isNotEmpty() },
                    plan = account.plan?.trim()?.takeIf { it.isNotEmpty() },
                    signedIn = account.signedIn,
                    health = AgentHealthRules.of(account),
                    active = true,
                    usage = null,
                ),
            )
        }
        return profiles.map { profile ->
            SessionAccountOption(
                profileId = profile.id,
                label = profile.label?.trim()?.takeIf { it.isNotEmpty() }
                    ?: if (profile.id == SYSTEM_PROFILE_ID) "Default" else profile.id,
                email = profile.email?.trim()?.takeIf { it.isNotEmpty() },
                plan = profile.plan?.trim()?.takeIf { it.isNotEmpty() },
                signedIn = profile.signedIn,
                health = AgentHealthRules.of(profile),
                active = profile.active,
                usage = profile.usage,
            )
        }
    }

    /**
     * Why [option] cannot be switched to right now, or null when it can.
     * Display gating only — the server and the machine re-check everything.
     *
     * [currentAccount] is the profile this run is KNOWN to be on, when the
     * client knows it (`coding_sessions.agent_account` is server-only, so it is
     * usually null and the machine's own active login is not a safe stand-in).
     */
    fun refusal(
        option: SessionAccountOption,
        agent: String?,
        mine: Boolean,
        sessionEnded: Boolean,
        deviceOnline: Boolean,
        canResume: Boolean,
        turnState: String,
        currentAccount: String? = null,
    ): String? = when {
        agent != SWITCHABLE_AGENT -> REASON_AGENT
        !mine -> REASON_NOT_MINE
        sessionEnded -> REASON_ENDED
        !deviceOnline -> REASON_OFFLINE
        !canResume -> REASON_NO_CAP
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
     * run is still live". `system` is accepted there explicitly and skips the
     * profile-membership check (web `session-account-switch.tsx` sends the
     * profile id verbatim too).
     */
    fun wireAccount(option: SessionAccountOption): String = option.profileId
}
