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
 * clamping (ultracode is claude-only, plan mode is claude/pi-only (EXP-441),
 * skip-permissions never applies to pi) stays the reader's job.
 */
@Serializable
data class AgentLaunchDefaults(
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean = false,
    @SerialName("planMode") val planMode: Boolean = false,
    @SerialName("skipPermissions") val skipPermissions: Boolean = false,
)

/**
 * A machine's per-agent coding defaults, advertised on the presence row so a
 * remote Start-coding sheet opens on the SAME settings the machine itself
 * would use (EXP-437). [agents] is keyed by contract `codingAgent` id and
 * covers only the RUNNABLE agents; [defaultAgent] is the machine's configured
 * default and must be clamped to what it can actually run. The whole field is
 * absent on an older desktop — readers fall back to the static contract
 * defaults.
 */
@Serializable
data class DeviceLaunchDefaults(
    @SerialName("defaultAgent") val defaultAgent: String? = null,
    @SerialName("agents") val agents: Map<String, AgentLaunchDefaults> = emptyMap(),
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
    // ── Team sharing (EXP-432) ───────────────────────────────────────────────
    /**
     * The team this machine is shared with, null when private. Carried on the
     * caller's OWN rows too. Since EXP-481 the share toggle lives in the
     * device-settings sheet here as well as on the web.
     */
    @SerialName("sharedTeamId") val sharedTeamId: String? = null,
    /** Set only on a TEAMMATE's shared machine — never on the caller's own. */
    @SerialName("owner") val owner: DeviceOwner? = null,
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

    /** EXP-409: agents installed but signed out — displayed, never offered. */
    val unauthedAgentIds: List<String>
        get() = DomainContract.codingAgentValues.filter { it in unauthedAgents }

    /**
     * Whether the machine can take a start at all (EXP-409) — an online
     * desktop whose every installed agent is signed out is as unstartable as
     * an offline one, so pickers drop it and the machines list explains why.
     */
    val hasRunnableAgent: Boolean get() = runnableAgents.isNotEmpty()

    /** Whether this desktop can run team actions (EXP-253). */
    val canRunActions: Boolean get() = caps?.contains("actions") == true

    /**
     * Whether this desktop understands typed action inputs + the builtin
     * "Create action" run (EXP-257) — required IN ADDITION to [canRunActions]
     * for builtin or inputs-carrying runs.
     */
    val canRunActionInputs: Boolean get() = caps?.contains("action-inputs") == true

    /**
     * Whether this desktop can run the builtin "Fix merge conflicts" action
     * (EXP-259). The server rejects that builtin without the cap, so pickers
     * filter such desktops out instead of failing after submit (EXP-323).
     */
    val canFixConflicts: Boolean get() = caps?.contains("fix-conflicts") == true

    /**
     * Whether this machine honors `resume` on a remote start (EXP-481).
     * Strictly cap-gated like actions — an old build would silently drop the
     * flag and start fresh, which reads as losing the session.
     */
    val canResume: Boolean get() = caps?.contains("resume") == true

    /**
     * Whether this machine runs action automations locally (EXP-530) — the
     * trigger device picker offers only these (offline-but-capable stays
     * pickable: the machine fires on its own clock once it's back).
     */
    val canRunAutomations: Boolean get() = caps?.contains("automations") == true

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
    @SerialName("codingSessionId") val codingSessionId: String,
)

@Serializable
private data class KillSessionInput(
    @SerialName("codingSessionId") val codingSessionId: String,
)

/**
 * Launch options a remote start may carry (EXP-149) — the Start-coding
 * sheet's choices. Null fields are omitted from the wire (the shared Json has
 * explicitNulls=false) and mean "desktop settings default" (plan mode OFF).
 * An empty [effort] is an explicit "CLI default" (omit --effort). A null
 * [agent] means claude (EXP-201); [skipPermissions] only applies to agents
 * with a guarded auto mode (claude/codex — pi is always unguarded).
 */
data class SteerStartOptions(
    val model: String? = null,
    val effort: String? = null,
    val ultracode: Boolean? = null,
    val planMode: Boolean? = null,
    val agent: String? = null,
    val skipPermissions: Boolean? = null,
    /**
     * EXP-481: resume the issue's existing worktree/agent session instead of
     * starting fresh. Single-issue starts only (the server rejects it on
     * batch/action forms), gated on [SteerDevice.canResume]; the batch and
     * action inputs simply never carry it.
     */
    val resume: Boolean? = null,
)

@Serializable
internal data class StartSessionInput(
    @SerialName("issueId") val issueId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("skipPermissions") val skipPermissions: Boolean? = null,
    @SerialName("resume") val resume: Boolean? = null,
)

// The batch form of steer.startSession (EXP-156): exactly one of
// issueId/issueIds — the [issueIds] variant launches ONE Claude session on ONE
// pushed `exp/batch-<id8>` branch that spans every listed issue (all in the
// same repository). Same endpoint + error mapping as the single-issue input.
@Serializable
private data class StartBatchSessionInput(
    @SerialName("issueIds") val issueIds: List<String>,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("skipPermissions") val skipPermissions: Boolean? = null,
)

// The action form of steer.startSession (EXP-253, widened by EXP-257):
// exactly one of issueId/issueIds/actionId — this variant runs a team action
// prompt on the trunk clone / a scratch dir. Action runs accept the FULL
// option set with the same per-agent vocabulary as issue runs; [teamId] is
// sent ONLY for the virtual builtin "Create action" id (the server requires
// it there and forbids it otherwise); [inputs] carries the filled input
// values keyed by def key (text, or a picked repo/board UUID). Null fields
// are omitted (explicitNulls=false) and mean "desktop settings default".
@Serializable
private data class StartActionSessionInput(
    @SerialName("actionId") val actionId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("teamId") val teamId: String? = null,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("skipPermissions") val skipPermissions: Boolean? = null,
    @SerialName("inputs") val inputs: Map<String, String>? = null,
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
     * best-effort fans a kill through the relay so the terminal tears down
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
     * may use: their own, or a teammate's server shared with the team (EXP-432). */
    suspend fun startSession(
        accountId: String,
        issueId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
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
                skipPermissions = options.skipPermissions,
                resume = options.resume,
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
                skipPermissions = options.skipPermissions,
            ),
            inputSerializer = StartBatchSessionInput.serializer(),
        )
    }

    /**
     * `steer.startSession` action form (EXP-253/EXP-257) — remote-run the
     * team action [actionId] on the picked online machine, which must
     * advertise the `actions` capability ([SteerDevice.canRunActions]; plus
     * `action-inputs` for builtin/inputs-carrying runs — the server enforces
     * both). [options] rides with the same per-agent vocabulary as the issue
     * forms; [teamId] must be passed ONLY for the builtin "Create action" id;
     * [inputs] are the filled input values keyed by def key. Same endpoint +
     * error mapping as the issue forms.
     */
    suspend fun startActionSession(
        accountId: String,
        actionId: String,
        deviceId: String,
        options: SteerStartOptions = SteerStartOptions(),
        teamId: String? = null,
        inputs: Map<String, String>? = null,
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
                skipPermissions = options.skipPermissions,
                inputs = inputs,
            ),
            inputSerializer = StartActionSessionInput.serializer(),
        )
    }
}

/**
 * The tRPC failure's user-presentable message, or [fallback] for anything
 * that isn't a [TrpcException]. Sanitization (server `message` extraction +
 * EXP-216 plan-cap neutralization) happens at the throw site in TrpcClient
 * (EXP-219), so the exception message is already safe to render.
 */
fun trpcErrorMessage(error: Throwable, fallback: String): String =
    (error as? TrpcException)?.message?.takeIf { it.isNotBlank() } ?: fallback
