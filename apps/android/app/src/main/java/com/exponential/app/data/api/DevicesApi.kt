package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/devices.ts. The EXP-403 registry is the
// caller's OWN machines — the desktop IDE and headless `exponential` daemon
// servers. Since EXP-481 the registry is server-authoritative synced state
// (the `devices` + `device_worktrees` shapes — see DeviceEntity); this API
// carries the curation mutations (rename/remove/update/share), the
// server-authoritative launch-defaults edit, and the owner→device command
// queue (durable rows the machine picks up on its heartbeat, online or not).
// The rows themselves come from sync; EXP-485 retired `devices.list` here for
// the informational `latestVersions` query.
//
// EXP-1043: this client no longer EMITS worktree commands — a machine's
// worktrees are a local surface, the IDE's own. The `worktree_remove` /
// `worktree_prune` kinds survive server-side only for machines still on an
// older build; EXP-1060 retires that wire once the version floors pass.

/**
 * Informational `CLIENT_LATEST_VERSION_*` values (null when unset
 * server-side) — a row whose [SteerDevice.version] compares below hints that
 * an update is available.
 */
@Serializable
data class DeviceLatestVersions(
    @SerialName("desktop") val desktop: String? = null,
    @SerialName("cli") val cli: String? = null,
)

@Serializable
private data class DeviceIdInput(@SerialName("deviceId") val deviceId: String)

@Serializable
private data class RenameDeviceInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("label") val label: String,
)

/**
 * `devices.setIcon` (EXP-924) — the owner-picked device glyph, or a literal
 * `null` to fall back to the kind default. A [JsonObject] rather than a
 * `@Serializable` class on purpose: the shared Json omits nulls
 * (`explicitNulls = false`), which would turn the reset into a no-op (the
 * `boards.update` branch-clear story).
 */
internal fun setDeviceIconInput(deviceId: String, icon: String?): JsonObject = buildJsonObject {
    put("deviceId", deviceId)
    put("icon", icon?.let(::JsonPrimitive) ?: JsonNull)
}

/** `devices.setDefault` (EXP-622) — flag/unflag the caller's default machine. */
@Serializable
private data class SetDefaultInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("isDefault") val isDefault: Boolean,
)

/** `devices.createCommand`'s answer — the queued row's id, for [DevicesApi.getCommand] polling. */
@Serializable
data class CreatedCommand(@SerialName("id") val id: String)

/**
 * One queued owner→device command (EXP-481) — an agent sign-in or a profile
 * switch, pending until the machine completes it. [result] carries the
 * device-reported message: the login URL, or the refusal reason on a `failed`
 * row.
 */
@Serializable
data class DeviceCommandDto(
    @SerialName("id") val id: String,
    @SerialName("kind") val kind: String = "",
    @SerialName("status") val status: String = STATUS_PENDING,
    @SerialName("result") val result: String? = null,
) {
    val isTerminal: Boolean get() = status == STATUS_DONE || status == STATUS_FAILED

    companion object {
        const val STATUS_PENDING = "pending"
        const val STATUS_DONE = "done"
        const val STATUS_FAILED = "failed"
    }
}

@Serializable
private data class CommandIdInput(@SerialName("commandId") val commandId: String)

/** `devices.setShared`'s toggle form (FEED-33): one team in or out. */
@Serializable
internal data class SetSharedInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("teamId") val teamId: String,
    @SerialName("shared") val shared: Boolean,
)

/** The shared Json's encode half (`HttpClientModule.provideJson`). */
private val launchDefaultsJson = Json {
    explicitNulls = false
    encodeDefaults = true
}

/**
 * `devices.setLaunchDefaults` input. Hand-built because an UNSET default
 * account beside a default agent must ride as a literal `defaultAccount: null`
 * (the clear), which the shared Json's `explicitNulls = false` would drop off
 * a `@Serializable` class: the server reads an ABSENT key as "an older client
 * that never sends it" and keeps the stored pin. Every other field encodes
 * exactly as [DeviceLaunchDefaults] always did; decoding is untouched.
 *
 * EXP-1043: that includes the `workflow` pair ([DeviceWorkflowDefaults]) —
 * present, it rides as its own object; absent, `explicitNulls = false` leaves
 * the KEY off, which the server reads as a client that predates the pair and
 * keeps the stored one rather than wiping it. The settings sheet therefore
 * sends the pair on EVERY save (the mutation replaces the whole object).
 */
internal fun setLaunchDefaultsInput(
    deviceId: String,
    defaults: DeviceLaunchDefaults,
): JsonObject {
    val encoded = launchDefaultsJson
        .encodeToJsonElement(DeviceLaunchDefaults.serializer(), defaults)
        .jsonObject
    val launchDefaults = if (defaults.defaultAgent != null && defaults.defaultAccount == null) {
        JsonObject(encoded + ("defaultAccount" to JsonNull))
    } else {
        encoded
    }
    return buildJsonObject {
        put("deviceId", deviceId)
        put("launchDefaults", launchDefaults)
    }
}

@Singleton
class DevicesApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `devices.latestVersions` (EXP-485) — the instance's informational
     * `CLIENT_LATEST_VERSION_*` values, the only thing the machine list still
     * needs from tRPC. Input-less: an empty object is dropped from the URL
     * entirely (TrpcClient's omitInputIfEmpty).
     */
    suspend fun latestVersions(accountId: String): DeviceLatestVersions =
        trpc.query(
            accountId,
            path = "devices.latestVersions",
            input = buildJsonObject { },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = DeviceLatestVersions.serializer(),
        )

    /** `devices.rename` — the registry label wins over what the relay holds. */
    suspend fun rename(accountId: String, deviceId: String, label: String) {
        trpc.mutationUnit(
            accountId,
            path = "devices.rename",
            input = RenameDeviceInput(deviceId = deviceId, label = label),
            inputSerializer = RenameDeviceInput.serializer(),
        )
    }

    /**
     * `devices.setIcon` (EXP-924) — the machine's display glyph, from the
     * device icon set (contract `deviceIcon`). [icon] = null RESETS it to the
     * kind default; the pick lands back through the devices shape.
     */
    suspend fun setIcon(accountId: String, deviceId: String, icon: String?) {
        trpc.mutationUnit(
            accountId,
            path = "devices.setIcon",
            input = setDeviceIconInput(deviceId, icon),
            inputSerializer = JsonObject.serializer(),
        )
    }

    /**
     * `devices.setDefault` (EXP-622) — make this machine the caller's default,
     * the row every device picker prefills. The server clears the flag on the
     * caller's other machines in the same transaction, so the result arrives
     * through the devices shape rather than this response.
     */
    suspend fun setDefault(accountId: String, deviceId: String, isDefault: Boolean) {
        trpc.mutationUnit(
            accountId,
            path = "devices.setDefault",
            input = SetDefaultInput(deviceId = deviceId, isDefault = isDefault),
            inputSerializer = SetDefaultInput.serializer(),
        )
    }

    /**
     * `devices.remove` — drops the registry row only; a machine whose daemon
     * still runs re-registers itself on its next heartbeat.
     */
    suspend fun remove(accountId: String, deviceId: String) {
        trpc.mutationUnit(
            accountId,
            path = "devices.remove",
            input = DeviceIdInput(deviceId = deviceId),
            inputSerializer = DeviceIdInput.serializer(),
        )
    }

    /**
     * `devices.requestUpdate` — flag a server daemon to self-update on its
     * next heartbeat. [SteerDevice.updateRequested] stays true until the
     * daemon re-registers, whether or not a newer build existed.
     */
    suspend fun requestUpdate(accountId: String, deviceId: String) {
        trpc.mutationUnit(
            accountId,
            path = "devices.requestUpdate",
            input = DeviceIdInput(deviceId = deviceId),
            inputSerializer = DeviceIdInput.serializer(),
        )
    }

    /**
     * `devices.setShared` (EXP-481, FEED-33): toggle ONE team in or out of
     * the share set of one of the caller's SERVER machines. The other teams
     * on the row are untouched; the synced `shared_team_ids` re-renders it.
     */
    suspend fun setShared(accountId: String, deviceId: String, teamId: String, shared: Boolean) {
        trpc.mutationUnit(
            accountId,
            path = "devices.setShared",
            input = SetSharedInput(deviceId = deviceId, teamId = teamId, shared = shared),
            inputSerializer = SetSharedInput.serializer(),
        )
    }

    /**
     * `devices.setLaunchDefaults` (EXP-481) — edit a machine's
     * server-authoritative per-agent coding defaults. Works with the machine
     * OFFLINE: the row is the truth and the machine's settings.json converges
     * on its next heartbeat (a relay nudge makes an online one immediate).
     * UI edits deliberately omit the device-push CAS stamp — unconditional
     * last-write-wins between humans.
     */
    suspend fun setLaunchDefaults(
        accountId: String,
        deviceId: String,
        defaults: DeviceLaunchDefaults,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "devices.setLaunchDefaults",
            input = setLaunchDefaultsInput(deviceId = deviceId, defaults = defaults),
            inputSerializer = JsonObject.serializer(),
        )
    }

    /**
     * `devices.createCommand` (EXP-481) — queue a command for the machine.
     * Durable: an OFFLINE machine runs it when it returns (the sheet says so
     * instead of blocking). Build the payload with [agentLoginCommand],
     * [agentLoginCodeCommand] or [agentProfileUseCommand] — the worktree
     * kinds are no longer emitted from here (see the file header).
     */
    suspend fun createCommand(
        accountId: String,
        command: JsonObject,
    ): CreatedCommand =
        trpc.mutation(
            accountId,
            path = "devices.createCommand",
            input = command,
            inputSerializer = JsonObject.serializer(),
            outputSerializer = CreatedCommand.serializer(),
        )

    /** `devices.getCommand` — the issuing UI's poll target while a command runs. */
    suspend fun getCommand(accountId: String, commandId: String): DeviceCommandDto =
        trpc.query(
            accountId,
            path = "devices.getCommand",
            input = CommandIdInput(commandId = commandId),
            inputSerializer = CommandIdInput.serializer(),
            outputSerializer = DeviceCommandDto.serializer(),
        )
}

/**
 * The `agent_login` input for [DevicesApi.createCommand] (EXP-484) — ask the
 * machine to run [agent]'s OWN sign-in flow and publish the login URL (plus
 * the codex device code) back as the command result. [switchAccount] signs the
 * current account out first. Gated on [SteerDevice.canAgentLogin].
 */
fun agentLoginCommand(
    deviceId: String,
    agent: String,
    switchAccount: Boolean,
    profileId: String? = null,
    /**
     * EXP-827/EXP-862: "Add account" — the machine CREATES a profile under
     * this label and runs the login in it. Mutually exclusive with
     * [profileId]; the server refuses both at once.
     */
    newProfileLabel: String? = null,
): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "agent_login")
        put("agent", agent)
        put("switch", switchAccount)
        // EXP-827/EXP-849: WHICH login on the machine this lands on — one of
        // `agentAccounts[agent].profiles` (`system` = the ambient login). Absent
        // means the ambient one.
        profileId?.trim()?.takeIf { it.isNotEmpty() }?.let { put("profileId", it) }
        newProfileLabel?.trim()?.takeIf { it.isNotEmpty() }
            ?.let { put("newProfileLabel", it.take(MAX_PROFILE_LABEL)) }
    }

/** The server clamps a profile label at 64 (web `MAX_PROFILE_LABEL`). */
const val MAX_PROFILE_LABEL = 64

/**
 * The `agent_profile_use` input for [DevicesApi.createCommand] (EXP-849) —
 * make the ALREADY-SIGNED-IN profile [profileId] the machine's ACTIVE login
 * for [agent] ("Set as default", EXP-862). Deliberately not a sign-in: no
 * credential is touched and nothing is signed out (a `codex logout` would
 * revoke the token server-wide), the machine just points itself at that
 * profile and re-reports `agent_accounts` on its next heartbeat, which is what
 * moves the chip's check. Gated on [SteerDevice.canAgentLogin] like the
 * sign-in, since it is the same machine capability.
 */
fun agentProfileUseCommand(deviceId: String, agent: String, profileId: String): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "agent_profile_use")
        put("agent", agent)
        put("profileId", profileId)
    }

/**
 * The `agent_profile_remove` input for [DevicesApi.createCommand] (EXP-862) —
 * the machine deletes ITS OWN copy of the login [profileId] for [agent]: the
 * agent CLI's config dir for that profile (credentials included) and its index
 * row. The ACCOUNT is untouched — no `codex logout` is ever run, which would
 * revoke it server-wide, and nothing about it leaves the machine.
 *
 * Gated on BOTH [SteerDevice.canAgentLogin] and [SteerDevice.canRemoveAccount]:
 * the server refuses the command without either cap, and the ambient login
 * (`system`) is refused outright — it is the CLI's own, not ours to delete.
 */
fun agentProfileRemoveCommand(deviceId: String, agent: String, profileId: String): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "agent_profile_remove")
        put("agent", agent)
        put("profileId", profileId)
    }

/**
 * The `agent_login_code` input for [DevicesApi.createCommand] (EXP-765) — hand
 * the authorization code the browser showed back to the sign-in still waiting
 * on the machine, which types it into that login's prompt. The server trims
 * [code] and refuses an empty one.
 */
fun agentLoginCodeCommand(deviceId: String, agent: String, code: String): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "agent_login_code")
        put("agent", agent)
        put("code", code)
    }

/**
 * The `agent_usage_refresh` input for [DevicesApi.createCommand] (EXP-747 C4,
 * EXP-829) — ask the machine to re-read [agent]'s usage for [profileId] past
 * the shared TTL (never past its own rate-limit floor: the server refuses a
 * machine without the `agent-usage-refresh` cap, and the device answers by
 * re-reporting on its next heartbeat, not through the command result).
 */
fun agentUsageRefreshCommand(deviceId: String, agent: String, profileId: String): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "agent_usage_refresh")
        put("agent", agent)
        put("profileId", profileId)
    }

/**
 * Whether [version] compares below [latest] (both `major.minor.patch`).
 * Missing or unparseable on either side = no hint, never a false alarm.
 * Mirrors `updateAvailable` in apps/web/src/components/my-machines.tsx.
 */
fun deviceUpdateAvailable(version: String?, latest: String?): Boolean {
    val have = parseVersionTuple(version) ?: return false
    val want = parseVersionTuple(latest) ?: return false
    for (i in 0 until 3) {
        if (have[i] != want[i]) return have[i] < want[i]
    }
    return false
}

// `major[.minor[.patch]]` with any `-rc1`/`+build` suffix stripped, missing
// components read as 0 — the web `parseVersionTuple` rules, so both clients
// hint on exactly the same version pairs.
private fun parseVersionTuple(version: String?): List<Int>? {
    val core = version?.trim()?.takeWhile { it != '-' && it != '+' } ?: return null
    val parts = core.split(".")
    val major = parts.getOrNull(0)?.toIntOrNull() ?: return null
    return listOf(major, parts.getOrNull(1)?.toIntOrNull() ?: 0, parts.getOrNull(2)?.toIntOrNull() ?: 0)
}
