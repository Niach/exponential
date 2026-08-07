package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/devices.ts. The EXP-403 registry is the
// caller's OWN machines — the desktop IDE and headless `exponential` daemon
// servers — kept per user over tRPC, deliberately NOT an Electric shape.
// `list` merges the durable rows (label, kind, last seen, version) with live
// relay presence, so a row carries both identity and startability; this app
// only reads and curates that list — registering is the machines' own job.
// EXP-432 widens `list` with an optional teamId: teammates' SERVER machines
// shared with that team come back too (read-only here — the share toggle is
// web-only), and a start on one runs there but is attributed to the caller.

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
