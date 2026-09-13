package com.exponential.app.data.api

import com.exponential.app.domain.DomainContract
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject

// Mirrors apps/web/src/lib/trpc/steer.ts (the ticket-minting router) + the
// relay wire contract in apps/steer-relay/src/protocol.ts. Android is a pure
// relay *client* (masterplan §5b/§5c): it remote-starts a coding session on
// one of the user's own online machines — or, since EXP-432, on a teammate's
// server shared with the team, where the run is still attributed to the
// requester — and watches/steers the live PTY over the relay socket. No local
// terminal, no CLI, no agent runtime.

/** Whether remote start + live steering is available on this instance. */
@Serializable
data class SteerConfigResult(
    val enabled: Boolean = false,
    @SerialName("relayUrl") val relayUrl: String? = null,
)

/**
 * The owner of a machine someone ELSE shares with the team (EXP-432) — set
 * only on the rows `devices.list({teamId})` appends after the caller's own,
 * so its absence IS "this one is mine" (see [SteerDevice.isMine]).
 */
@Serializable
data class DeviceOwner(
    @SerialName("id") val id: String,
    @SerialName("name") val name: String,
)

/**
 * One agent's launch defaults as a machine has them configured (EXP-437).
 * [model]/[effort] are contract values where an EMPTY string is the explicit
 * "CLI default" (omit the flag) — the same convention the start options use.
 * The booleans ride only when true, so an absent one IS false; capability
 * clamping (ultracode and plan mode are claude-only since EXP-849)
 * stays the reader's job.
 */
@Serializable
data class AgentLaunchDefaults(
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean = false,
    @SerialName("planMode") val planMode: Boolean = false,
)

/**
 * A machine's per-agent coding defaults, advertised on the presence row so a
 * remote Start-coding sheet opens on the SAME settings the machine itself
 * would use (EXP-437). [agents] is keyed by contract `codingAgent` id and
 * covers only the RUNNABLE agents; [defaultAgent] is the machine's configured
 * default and must be clamped to what it can actually run. The whole field is
 * absent on an older desktop — readers fall back to the static contract
 * defaults.
 *
 * EXP-773 dropped `startInTerminal` with the PTY coding path. An older server
 * still stamps that key; both decoders here are `ignoreUnknownKeys`, so it is
 * simply skipped instead of failing the whole object.
 */
@Serializable
data class DeviceLaunchDefaults(
    @SerialName("defaultAgent") val defaultAgent: String? = null,
    @SerialName("agents") val agents: Map<String, AgentLaunchDefaults> = emptyMap(),
)

/**
 * EXP-484: what a machine knows about ONE agent CLI's local sign-in. Read-only
 * status — no credential is ever carried, copied or refreshed. [plan] is the
 * subscription tier for claude/codex (`api key` for a codex API-key account);
 * an account with no email at all reports `"<provider> (oauth|api key)"`.
 * [checkedAt] is when the machine last probed.
 */
@Serializable
data class AgentAccount(
    @SerialName("signedIn") val signedIn: Boolean = false,
    @SerialName("email") val email: String? = null,
    @SerialName("plan") val plan: String? = null,
    @SerialName("checkedAt") val checkedAt: String? = null,
    /**
     * EXP-849: how USABLE the login is, as the machine's USAGE PROBE saw it
     * (`auth status` is identity only, never health) — one of
     * [AgentHealth]'s wire tokens. Absent on a pre-EXP-849 device: derive it
     * from [signedIn] (`AgentHealthRules.of`). A value this build has no name
     * for reads as `unknown`, never blanks the row.
     */
    @SerialName("health") val health: String? = null,
    /**
     * EXP-825: the agent's login PROFILES on the machine (web
     * `agentAccounts[agent].profiles`) — the composer's Account picker offers
     * them when there are two or more. Absent = the device reported none.
     */
    @SerialName("profiles") val profiles: List<AgentAccountProfile>? = null,
)

/**
 * EXP-825: one login profile of an agent on a machine — [id] is what a start
 * sends as `account`, [active] marks the machine's current login, [email]
 * names it. Everything but the id is optional (the sender's vintage varies),
 * and the shared decoder ignores unknown keys, so a richer profile from a
 * newer desktop still decodes.
 *
 * EXP-829: the Devices page's Accounts section reads the rest of the entry
 * too (web `DeviceAgentProfileEntry`): [label] for the chip, [signedIn] /
 * [plan] / [checkedAt] for the identity line, and the profile's OWN [usage]
 * — the active profile's numbers also ride the pre-profile `agentUsage`
 * slot, which is the fallback for a device that only populated that one.
 */
@Serializable
data class AgentAccountProfile(
    @SerialName("id") val id: String,
    @SerialName("active") val active: Boolean = false,
    @SerialName("email") val email: String? = null,
    @SerialName("label") val label: String? = null,
    @SerialName("signedIn") val signedIn: Boolean = false,
    @SerialName("plan") val plan: String? = null,
    @SerialName("checkedAt") val checkedAt: String? = null,
    /** EXP-849: this profile's own health — see [AgentAccount.health]. */
    @SerialName("health") val health: String? = null,
    @SerialName("usage") val usage: AgentUsage? = null,
)

/**
 * The web's `SYSTEM_PROFILE_ID`: the machine's ambient login, which a start
 * never names explicitly — a picked "Active login" sends no `account`.
 */
const val SYSTEM_PROFILE_ID = "system"

/**
 * One rate-limit window of an agent's usage (EXP-484). [key] is stable
 * (`session`, `weekly`, `model:<name>`, `credits`, `<durationMins>`) and is
 * what a per-agent window preference stores; [label] is the display string
 * (`5h`, `Week`, `Fable`, `Credits`, `Month`). [percent] is 0-100 as reported.
 */
@Serializable
data class AgentUsageWindow(
    @SerialName("key") val key: String,
    @SerialName("label") val label: String = "",
    @SerialName("percent") val percent: Double = 0.0,
    @SerialName("resetsAt") val resetsAt: String? = null,
)

/**
 * One agent's usage snapshot (EXP-484). [fetchedAt] gates rendering (older
 * than 15 minutes = not shown, fail closed) and [stale] means the numbers are
 * the last good ones after a failed refresh — rendered dimmed with an
 * "as of ..." caption rather than dropped.
 */
@Serializable
data class AgentUsage(
    @SerialName("fetchedAt") val fetchedAt: String? = null,
    @SerialName("stale") val stale: Boolean = false,
    @SerialName("windows") val windows: List<AgentUsageWindow> = emptyList(),
)

/**
 * One machine the caller can start on (mirrors web's `lib/steer-devices.ts`).
 * Every surface reads these from `devices.list` (EXP-403 — the durable
 * registry merged with live relay presence, plus the team's shared servers
 * since EXP-432), but the shape is the RELAY's too: the registry fields are
 * all defaulted, so a relay-only row decodes unchanged and reads as an online
 * desktop.
 *
 * [agents] lists the coding agents the machine can RUN (EXP-201; since
 * EXP-409 that means installed AND signed in) — an ABSENT list is an older
 * sender that only runs claude, but an explicitly EMPTY one means the machine
 * can run nothing right now, so the fallback must never fire on it (see
 * [runnableAgents]). [unauthedAgents] carries the agents installed but SIGNED
 * OUT — never startable, only shown as the reason. [caps] lists feature
 * capabilities (EXP-253: `actions`; EXP-257: `action-inputs`) — absent (old
 * desktop/relay) means none: action starts are strictly gated on it, unlike
 * the lenient agents fallback.
 */
@Serializable
data class SteerDevice(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("deviceLabel") val deviceLabel: String = "",
    @SerialName("connectedAt") val connectedAt: Long = 0,
    @SerialName("agents") val agents: List<String>? = null,
    @SerialName("unauthedAgents") val unauthedAgents: List<String> = emptyList(),
    @SerialName("caps") val caps: List<String>? = null,
    /**
     * EXP-749: the agents this machine runs through the in-process ACP engine
     * (the session screen). Every build above the version floor reports it, so
     * an EMPTY list is the real answer — that machine can start nothing right
     * now. Decoding stays tolerant of a missing key: `devices.acp_agents` is a
     * nullable column and a registry row for a machine that never reported
     * still syncs as NULL (DeviceRows maps that to empty).
     */
    @SerialName("acpAgents") val acpAgents: List<String> = emptyList(),
    /**
     * EXP-437: the machine's per-agent coding defaults, so a remote start
     * pre-fills what that machine would use locally. Absent on older desktops
     * (and on a machine with nothing runnable) — the sheet then falls back to
     * the static contract defaults.
     */
    @SerialName("launchDefaults") val launchDefaults: DeviceLaunchDefaults? = null,
    // ── devices.list registry fields (EXP-403) ───────────────────────────────
    /** `desktop` (the IDE) or `server` (a headless `exponential` daemon). */
    @SerialName("kind") val kind: String = KIND_DESKTOP,
    @SerialName("platform") val platform: String? = null,
    /** Connected to the relay right now. Relay-only rows omit it and ARE online. */
    @SerialName("online") val online: Boolean = true,
    /** ISO timestamp of the last register/heartbeat; null on a relay-only row. */
    @SerialName("lastSeenAt") val lastSeenAt: String? = null,
    /** Backed by a registry row — rename/remove only exist for those. */
    @SerialName("registered") val registered: Boolean = false,
    /** Marketing version as of the last register; null for old builds. */
    @SerialName("version") val version: String? = null,
    /** An Update request is pending on the daemon (cleared when it re-registers). */
    @SerialName("updateRequested") val updateRequested: Boolean = false,
    /**
     * EXP-411: the pending update is parked behind live coding sessions —
     * the daemon applies it once they close ("Update queued", no spinner).
     */
    @SerialName("updateBlocked") val updateBlocked: Boolean = false,
    // ── Team sharing (EXP-432, FEED-33) ──────────────────────────────────────
    /**
     * The teams this machine is shared with, empty when private. Carried on
     * the caller's OWN rows too. Since EXP-481 the per-team share toggles
     * live in the device-settings sheet here as well as on the web.
     */
    @SerialName("sharedTeamIds") val sharedTeamIds: List<String> = emptyList(),
    /** Set only on a TEAMMATE's shared machine — never on the caller's own. */
    @SerialName("owner") val owner: DeviceOwner? = null,
    /**
     * EXP-622: the caller's DEFAULT machine — pickers prefill it over their
     * first candidate. Set only on the caller's own rows: the flag lives on
     * the device row and belongs to its owner, so a teammate's shared server
     * never prefills off it.
     */
    @SerialName("isDefault") val isDefault: Boolean = false,
    // ── Agent auth + usage status (EXP-484) ──────────────────────────────────
    /**
     * Per-agent sign-in status keyed by contract `codingAgent` id, as the
     * machine last probed it. Absent on relay-only rows and on machines older
     * than EXP-484 — the Agents section then simply doesn't render.
     */
    @SerialName("agentAccounts") val agentAccounts: Map<String, AgentAccount>? = null,
    /** Per-agent usage windows keyed the same way; absent = nothing to show. */
    @SerialName("agentUsage") val agentUsage: Map<String, AgentUsage>? = null,
    /** Server stamp of the last usage write — the offline "as of ..." fallback. */
    @SerialName("agentUsageAt") val agentUsageAt: String? = null,
    /**
     * EXP-481: the synced `devices` ROW id — joins `device_worktrees` rows.
     * Null on rows decoded from `devices.list` / relay payloads (which never
     * carry it); only the DeviceEntity → SteerDevice mapping stamps it.
     */
    @SerialName("rowId") val rowId: String? = null,
) {
    /** A headless `exponential` daemon rather than the desktop IDE. */
    val isServer: Boolean get() = kind == KIND_SERVER

    /**
     * EXP-432: one of the caller's own machines. Teammates' shared rows carry
     * an [owner]; own rows never do — the same test web's `deviceIsMine` makes.
     */
    val isMine: Boolean get() = owner == null

    /** EXP-411: the pending update waits for live sessions to close. */
    val updateQueued: Boolean get() = updateRequested && updateBlocked

    /**
     * The agents this machine can launch right now, in contract order. An
     * ABSENT advertisement is a pre-EXP-201 sender that runs exactly claude;
     * an explicitly EMPTY one is a machine with nothing runnable (EXP-409 —
     * every installed agent is signed out), and stays empty. Mirrors web's
     * `deviceAgentIds`.
     */
    val runnableAgents: List<String>
        get() = agents?.let { advertised ->
            DomainContract.codingAgentValues.filter { it in advertised }
        } ?: listOf(FALLBACK_AGENT)

    /**
     * The agents this machine runs through the ACP engine, in contract order
     * (EXP-749) — see [acpAgents]. Empty = nothing can start there.
     */
    val acpAgentIds: List<String>
        get() = DomainContract.codingAgentValues.filter { it in acpAgents }

    /**
     * EXP-773: whether [agent] CANNOT start here — it is outside the machine's
     * ACP set, and there is no PTY path left. Nothing is FILTERED on it: the
     * agent stays pickable, the caption says why the start is blocked.
     */
    fun agentNotReady(agent: String): Boolean = agent !in acpAgentIds

    /** EXP-409: agents installed but signed out — displayed, never offered. */
    val unauthedAgentIds: List<String>
        get() = DomainContract.codingAgentValues.filter { it in unauthedAgents }

    /**
     * Whether the machine can take a start at all (EXP-409) — an online
     * desktop whose every installed agent is signed out is as unstartable as
     * an offline one, so pickers drop it and the machines list explains why.
     */
    val hasRunnableAgent: Boolean get() = runnableAgents.isNotEmpty()

    // EXP-672: the `actions` / `action-inputs` / `fix-conflicts` / `chat` /
    // `resume` mirrors are GONE. Every desktop and CLI above the version floor
    // advertises all five whenever it advertises a runnable agent, and the
    // server stopped refusing on them (EXP-624/EXP-639) — the mirrors gated on
    // nothing and only made the pickers lie ("update the desktop app") when the
    // real reason was that no machine was online. What remains here mirrors a
    // gate the server still applies.

    /**
     * EXP-637: whether this machine can RESUME an ended action/chat run — it
     * still holds the run's worktree and agent transcript, so the resumed
     * session picks up where the last one stopped. Cap-gated like the others:
     * the server refuses a resume start without it, so pickers hide the
     * affordance instead of failing after the tap.
     */
    val canResumeRun: Boolean get() = caps?.contains("resume-run") == true

    /**
     * Whether this machine runs action automations locally (EXP-530) — the
     * trigger device picker offers only these (offline-but-capable stays
     * pickable: the machine fires on its own clock once it's back).
     */
    val canRunAutomations: Boolean get() = caps?.contains("automations") == true

    /**
     * EXP-484: whether this machine can run an agent sign-in on request — the
     * `agent_login` device command opens the CLI's own login flow there and
     * publishes the URL (plus the codex device code) back. Cap-gated: an older
     * build would leave the command pending forever, so the Login / Switch
     * account buttons only appear for machines that advertise it.
     */
    val canAgentLogin: Boolean get() = caps?.contains("agent-login") == true

    /**
     * EXP-849: whether this machine can switch the account of a LIVE run —
     * end it, move the transcript into the other profile and relaunch there.
     * A build without the cap resumes on the RECORDED account and drops the
     * `account` field, so the switch would silently keep the exhausted login;
     * the server refuses such a live switch, and the rows say so instead of
     * failing after the tap. An ENDED run needs no cap: naming an account on
     * a resume is the launch-time choice every build honours.
     */
    val canSwitchAccount: Boolean get() = caps?.contains("account-switch") == true

    /**
     * EXP-862: whether this machine can REMOVE one of its agent logins
     * (`agent_profile_remove` — it deletes its own copy of the profile, never
     * the account). Cap-gated like the switch: the server refuses the command
     * without it, so the chip menu simply does not offer the entry on an older
     * build instead of queueing something that would never run.
     */
    val canRemoveAccount: Boolean get() = caps?.contains("account-remove") == true

    /**
     * EXP-746: whether this machine runs coding sessions through the
     * in-process ACP engine. Read-only here and no longer a gate: every build
     * above the version floor advertises it (EXP-773 left no other transport),
     * so nothing filters on it.
     */
    val supportsAcp: Boolean get() = caps?.contains("acp") == true

    companion object {
        const val KIND_DESKTOP = "desktop"
        const val KIND_SERVER = "server"

        /** What a sender that advertises no agents at all can run. */
        private const val FALLBACK_AGENT = "claude"
    }
}

/**
 * A minted relay ticket + the full `ws(s)://<relay>/ws?ticket=<token>` dial
 * URL. `disabled == true` (or nil ticket/url) means steer is off on this
 * instance — a result, not an error.
 */
@Serializable
data class SteerTicketResult(
    val ticket: String? = null,
    val url: String? = null,
    val disabled: Boolean = false,
) {
    val isUsable: Boolean get() = !disabled && !ticket.isNullOrBlank() && !url.isNullOrBlank()
}

@Serializable
private data class ViewerTicketInput(
    val kind: String = "viewer",
    @SerialName("sessionId") val codingSessionId: String,
)

@Serializable
private data class KillSessionInput(
    @SerialName("sessionId") val codingSessionId: String,
)

/**
 * Launch options a remote start may carry (EXP-149) — the Start-coding
 * sheet's choices. Null fields are omitted from the wire (the shared Json has
 * explicitNulls=false) and mean "desktop settings default" (plan mode OFF).
 * An empty [effort] is an explicit "CLI default" (omit --effort). A null
 * [agent] means claude (EXP-201).
 */
data class SteerStartOptions(
    val model: String? = null,
    val effort: String? = null,
    val ultracode: Boolean? = null,
    val planMode: Boolean? = null,
    val agent: String? = null,
    /**
     * EXP-481: resume the issue's existing worktree/agent session instead of
     * starting fresh. Single-issue starts only (the server rejects it on
     * batch/action forms); the batch and action inputs simply never carry it.
     */
    val resume: Boolean? = null,
    /**
     * EXP-825 (EXP-792): the agent login profile to launch under — one of the
     * machine's `agentAccounts[agent].profiles` ids. Null = the machine's
     * active login (never [SYSTEM_PROFILE_ID] on the wire).
     */
    val account: String? = null,
)

// The three non-resume forms of steer.startSession all carry the EXP-825
// `prompt` LAST: the composer's free text (with its image embeds) — the chat
// message for the Chat builtin, the request for Create action, additional
// instructions on everything else. Null is omitted (explicitNulls=false), so
// the server sees NO prompt rather than an empty one.
@Serializable
internal data class StartSessionInput(
    @SerialName("issueId") val issueId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("resume") val resume: Boolean? = null,
    @SerialName("account") val account: String? = null,
    @SerialName("prompt") val prompt: String? = null,
)

// The batch form of steer.startSession (EXP-156): exactly one of
// issueId/issueIds — the [issueIds] variant launches ONE Claude session on ONE
// pushed `exp/batch-<id8>` branch that spans every listed issue (all in the
// same repository). Same endpoint + error mapping as the single-issue input.
@Serializable
internal data class StartBatchSessionInput(
    @SerialName("issueIds") val issueIds: List<String>,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("account") val account: String? = null,
    @SerialName("prompt") val prompt: String? = null,
)

// The action form of steer.startSession (EXP-253, widened by EXP-257):
// exactly one of issueId/issueIds/actionId — this variant runs a team action
// prompt on the trunk clone / a scratch dir. Action runs accept the FULL
// option set with the same per-agent vocabulary as issue runs; [teamId] is
// sent ONLY for the virtual builtin "Create action" id (the server requires
// it there and forbids it otherwise); [inputs] carries the filled input
// values keyed by def key (a picked repo/board/pr id or icon name — EXP-825
// retired free-text inputs; the text is [prompt]). Null fields are omitted
// (explicitNulls=false) and mean "desktop settings default".
@Serializable
internal data class StartActionSessionInput(
    @SerialName("actionId") val actionId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("teamId") val teamId: String? = null,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("inputs") val inputs: Map<String, String>? = null,
    @SerialName("account") val account: String? = null,
    @SerialName("prompt") val prompt: String? = null,
)

// The resume form of steer.startSession (EXP-637): exactly one of
// issueId/issueIds/actionId/resumeSessionId — this variant continues an ENDED
// run on the machine that ran it. A resumed run keeps its recorded agent and
// options, so no launch option rides along; the server refuses a run that is
// still live, one owned by someone else, and a device that is offline or
// lacks the `resume-run` cap.
@Serializable
private data class ResumeSessionInput(
    @SerialName("resumeSessionId") val resumeSessionId: String,
    @SerialName("deviceId") val deviceId: String,
    /**
     * EXP-849 phase 3: the login the resumed run re-enters under — a remote
     * "switch account" IS a resume that names an account. Null = the run's own
     * recorded account (the plain Resume), and the ambient login is never named
     * (`system` is the absence of the field).
     */
    @SerialName("account") val account: String? = null,
)

@Singleton
class SteerApi @Inject constructor(private val trpc: TrpcClient) {

    /** `steer.config` — enabled iff the server has a relay configured. */
    suspend fun config(accountId: String): SteerConfigResult =
        trpc.query(
            accountId,
            path = "steer.config",
            input = buildJsonObject { },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = SteerConfigResult.serializer(),
        )

    /** `steer.mintTicket({kind:'viewer'})` — watch/steer a running session. */
    suspend fun mintViewerTicket(accountId: String, codingSessionId: String): SteerTicketResult =
        trpc.mutation(
            accountId,
            path = "steer.mintTicket",
            input = ViewerTicketInput(codingSessionId = codingSessionId),
            inputSerializer = ViewerTicketInput.serializer(),
            outputSerializer = SteerTicketResult.serializer(),
        )

    /**
     * `steer.killSession` (EXP-268) — force-end a running session: the server
     * flips the synced coding_sessions row to `ended` (the desktop watches its
     * own row, so this aborts the run even with the relay unreachable) and
     * best-effort fans a kill through the relay so the run tears down
     * immediately. Owner-or-team-owner gated server-side; idempotent.
     */
    suspend fun killSession(accountId: String, codingSessionId: String) {
        trpc.mutationUnit(
            accountId,
            path = "steer.killSession",
            input = KillSessionInput(codingSessionId = codingSessionId),
            inputSerializer = KillSessionInput.serializer(),
        )
    }

    /** `steer.startSession` — remote-start on any online machine the caller
     * may use: their own, or a teammate's server shared with the team (EXP-432).
     * [prompt] (EXP-825) is the composer's optional additional instructions. */
    suspend fun startSession(
        accountId: String,
        issueId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        prompt: String? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "steer.startSession",
            input = StartSessionInput(
                issueId = issueId,
                deviceId = deviceId,
                model = options.model,
                effort = options.effort,
                ultracode = options.ultracode,
                planMode = options.planMode,
                agent = options.agent,
                resume = options.resume,
                account = options.account,
                prompt = prompt,
            ),
            inputSerializer = StartSessionInput.serializer(),
        )
    }

    /**
     * `steer.startSession` batch form — remote-start ONE session spanning
     * [issueIds] (2+; all in the same repository) on the picked machine.
     * Same endpoint + error mapping as the single-issue [startSession].
     */
    suspend fun startSession(
        accountId: String,
        issueIds: List<String>,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        prompt: String? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "steer.startSession",
            input = StartBatchSessionInput(
                issueIds = issueIds,
                deviceId = deviceId,
                model = options.model,
                effort = options.effort,
                ultracode = options.ultracode,
                planMode = options.planMode,
                agent = options.agent,
                account = options.account,
                prompt = prompt,
            ),
            inputSerializer = StartBatchSessionInput.serializer(),
        )
    }

    /**
     * `steer.startSession` resume form (EXP-637) — continue the ENDED run
     * [sessionId] on [deviceId], the machine that ran it (it still holds the
     * worktree and the agent's transcript). Owner-only server-side, and gated
     * on [SteerDevice.canResumeRun]; the resumed run keeps its recorded agent
     * and options, so nothing else rides along. Same error mapping as the
     * other forms.
     *
     * EXP-849 phase 3: [account] makes this a mid-session ACCOUNT SWITCH — the
     * machine re-enters the recorded run under that login (claude only), in the
     * same worktree, and links the new row by `resumed_from_id` so it presents
     * as a continuation.
     */
    suspend fun resumeSession(
        accountId: String,
        sessionId: String,
        deviceId: String,
        account: String? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "steer.startSession",
            input = ResumeSessionInput(
                resumeSessionId = sessionId,
                deviceId = deviceId,
                account = account,
            ),
            inputSerializer = ResumeSessionInput.serializer(),
        )
    }

    /**
     * `steer.startSession` action form (EXP-253/EXP-257) — remote-run the
     * team action [actionId] on the picked machine: an online machine with a
     * runnable agent is the whole requirement (EXP-672: the action capability
     * refusals are gone server-side, the agent check is the one that
     * remains). [options] rides with the same per-agent vocabulary as the issue
     * forms; [teamId] must be passed ONLY for the builtin "Create action" id;
     * [inputs] are the filled input values keyed by def key; [prompt]
     * (EXP-825) is the composer's text — REQUIRED by the server for the Chat
     * and Create action builtins, additional instructions otherwise. Same
     * endpoint + error mapping as the issue forms.
     */
    suspend fun startActionSession(
        accountId: String,
        actionId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        teamId: String? = null,
        inputs: Map<String, String>? = null,
        prompt: String? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "steer.startSession",
            input = StartActionSessionInput(
                actionId = actionId,
                deviceId = deviceId,
                teamId = teamId,
                model = options.model,
                effort = options.effort,
                ultracode = options.ultracode,
                planMode = options.planMode,
                agent = options.agent,
                inputs = inputs,
                account = options.account,
                prompt = prompt,
            ),
            inputSerializer = StartActionSessionInput.serializer(),
        )
    }
}
