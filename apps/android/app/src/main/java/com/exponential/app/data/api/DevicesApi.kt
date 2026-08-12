package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/devices.ts. The EXP-403 registry is the
// caller's OWN machines — the desktop IDE and headless `exponential` daemon
// servers. Since EXP-481 the registry is server-authoritative synced state
// (the `devices` + `device_worktrees` shapes — see DeviceEntity); this API
// carries the curation mutations (rename/remove/update/share), the
// server-authoritative launch-defaults edit, and the owner→device worktree
// command queue (remove/prune — durable rows the machine picks up on its
// heartbeat, online or not). `list` survives ONLY as the latestVersions
// source; the rows themselves come from sync.

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
data class DeviceListResult(
    @SerialName("devices") val devices: List<SteerDevice> = emptyList(),
    @SerialName("latestVersions") val latestVersions: DeviceLatestVersions = DeviceLatestVersions(),
)

@Serializable
private data class DeviceIdInput(@SerialName("deviceId") val deviceId: String)

@Serializable
private data class RenameDeviceInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("label") val label: String,
)

/** `devices.createCommand`'s answer — the queued row's id, for [DevicesApi.getCommand] polling. */
@Serializable
data class CreatedCommand(@SerialName("id") val id: String)

/**
 * One queued owner→device command (EXP-481): `worktree_remove` /
 * `worktree_prune`, pending until the machine completes it. [result] carries
 * the device-reported message — the prune summary, or the refusal reason on a
 * `failed` row.
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

@Serializable
private data class SetLaunchDefaultsInput(
    @SerialName("deviceId") val deviceId: String,
    @SerialName("launchDefaults") val launchDefaults: DeviceLaunchDefaults,
)

@Singleton
class DevicesApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `devices.list` — the caller's machines, online and offline. With
     * [teamId] the response ALSO carries teammates' server machines shared
     * with that team (EXP-432), appended after the caller's own rows and
     * marked by their [SteerDevice.owner]. Omitting it keeps the pre-EXP-432
     * own-machines-only behavior (the input is optional server-side).
     */
    suspend fun list(accountId: String, teamId: String? = null): DeviceListResult =
        trpc.query(
            accountId,
            path = "devices.list",
            // An empty object is dropped from the URL entirely (TrpcClient's
            // omitInputIfEmpty), which is exactly what the no-team call means.
            input = buildJsonObject { if (teamId != null) put("teamId", teamId) },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = DeviceListResult.serializer(),
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
     * `devices.setShared` (EXP-481 — the toggle was web-only before): share
     * one of the caller's SERVER machines with [teamId], or clear the share
     * with null. The server requires the `teamId` KEY to be present even for
     * clearing, and the shared Json drops nulls (explicitNulls = false) — so
     * the input is built by hand with an explicit [JsonNull].
     */
    suspend fun setShared(accountId: String, deviceId: String, teamId: String?) {
        trpc.mutationUnit(
            accountId,
            path = "devices.setShared",
            input = setSharedInput(deviceId, teamId),
            inputSerializer = JsonObject.serializer(),
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
            input = SetLaunchDefaultsInput(deviceId = deviceId, launchDefaults = defaults),
            inputSerializer = SetLaunchDefaultsInput.serializer(),
        )
    }

    /**
     * `devices.createCommand` (EXP-481) — queue a worktree command for the
     * machine. Durable: an OFFLINE machine runs it when it returns (the sheet
     * says so instead of blocking). Build the payload with
     * [worktreeRemoveCommand] / [worktreePruneCommand].
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
 * `devices.setShared`'s input. Built by hand: the server requires the
 * `teamId` KEY even when clearing, and the shared Json drops nulls
 * (explicitNulls = false) — a synthesized Encodable would silently turn
 * "stop sharing" into a no-op BAD_REQUEST.
 */
internal fun setSharedInput(deviceId: String, teamId: String?): JsonObject = buildJsonObject {
    put("deviceId", deviceId)
    put("teamId", teamId?.let(::JsonPrimitive) ?: JsonNull)
}

/** The `worktree_remove` input for [DevicesApi.createCommand]. */
fun worktreeRemoveCommand(deviceId: String, repoFullName: String, branch: String): JsonObject =
    buildJsonObject {
        put("deviceId", deviceId)
        put("kind", "worktree_remove")
        put("repoFullName", repoFullName)
        put("branch", branch)
    }

/** The `worktree_prune` input for [DevicesApi.createCommand]. */
fun worktreePruneCommand(deviceId: String): JsonObject = buildJsonObject {
    put("deviceId", deviceId)
    put("kind", "worktree_prune")
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
