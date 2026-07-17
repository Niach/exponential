package com.exponential.app.data.api

import java.util.Base64
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

// Mirrors apps/web/src/lib/trpc/steer.ts (the ticket-minting router) + the
// relay wire contract in apps/steer-relay/src/protocol.ts. Android is a pure
// relay *client* (masterplan §5b/§5c): it remote-starts a coding session on
// the user's own online desktop and watches/steers the live PTY over the
// relay socket — no local terminal, no CLI, no agent runtime.

/** Whether remote start + live steering is available on this instance. */
@Serializable
data class SteerConfigResult(
    val enabled: Boolean = false,
    @SerialName("relayUrl") val relayUrl: String? = null,
)

/** One of the caller's online desktops (relay presence, no DB table). */
@Serializable
data class SteerDevice(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("deviceLabel") val deviceLabel: String = "",
    @SerialName("connectedAt") val connectedAt: Long = 0,
)

@Serializable
data class SteerDevicesResult(val devices: List<SteerDevice> = emptyList())

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

/**
 * Launch options a remote start may carry (EXP-149) — the Start-coding
 * sheet's choices. Null fields are omitted from the wire (the shared Json has
 * explicitNulls=false) and mean "desktop settings default" (plan mode OFF).
 * An empty [effort] is an explicit "CLI default" (omit --effort).
 */
data class SteerStartOptions(
    val model: String? = null,
    val effort: String? = null,
    val ultracode: Boolean? = null,
    val planMode: Boolean? = null,
)

@Serializable
private data class StartSessionInput(
    @SerialName("issueId") val issueId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("ultracode") val ultracode: Boolean? = null,
    @SerialName("planMode") val planMode: Boolean? = null,
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

    /** `steer.myDevices` — the caller's online desktops (device picker). */
    suspend fun myDevices(accountId: String): SteerDevicesResult =
        trpc.query(
            accountId,
            path = "steer.myDevices",
            input = buildJsonObject { },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = SteerDevicesResult.serializer(),
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

    /** `steer.startSession` — remote-start on the user's own online desktop. */
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
            ),
            inputSerializer = StartSessionInput.serializer(),
        )
    }

    /**
     * `steer.startSession` batch form — remote-start ONE session spanning
     * [issueIds] (2+; all in the same repository) on the user's own desktop.
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
            ),
            inputSerializer = StartBatchSessionInput.serializer(),
        )
    }
}

/**
 * The relay ticket is `base64url(JSON claims).base64url(sig)`; the claims
 * carry the caller's perm (`view`|`steer`), which decides whether steering
 * controls show. Decoding locally is display-only — the relay enforces perm
 * server-side (mirrors the web viewer's decodeTicketPerm).
 */
fun decodeSteerTicketPerm(ticket: String): String {
    return try {
        val payload = ticket.substringBefore('.')
        val bytes = Base64.getUrlDecoder().decode(payload)
        val claims = Json.parseToJsonElement(bytes.decodeToString()).jsonObject
        if ((claims["perm"] as? JsonPrimitive)?.contentOrNull == "steer") "steer" else "view"
    } catch (_: Throwable) {
        "view"
    }
}

/**
 * Extract the human-readable message from a tRPC error body ([TrpcException]
 * messages embed the raw response, e.g. PRECONDITION_FAILED's "No repository
 * linked to this project…"). Falls back to [fallback] on anything unparsable.
 */
fun trpcErrorMessage(error: Throwable, fallback: String): String {
    val raw = (error as? TrpcException)?.message ?: return fallback
    val jsonStart = raw.indexOf('{')
    if (jsonStart >= 0) {
        runCatching {
            val envelope = Json.parseToJsonElement(raw.substring(jsonStart)).jsonObject
            val err = envelope["error"]?.jsonObject ?: return@runCatching
            val payload = (err["json"] as? JsonObject) ?: err
            val message = (payload["message"] as? JsonPrimitive)?.contentOrNull
            if (!message.isNullOrBlank()) return message
        }
    }
    return fallback
}
