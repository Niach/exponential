package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject

// EXP-1196/1218/1219: THE device readiness block. A device builds `doctor`
// (desktop/CLI `coding::device_doctor`) and reports it on register +
// heartbeat; the server syncs it in the nullable `devices.doctor` jsonb.
// Every client renders the SAME block from it — the spec, labels, states,
// actions and cases live in `packages/domain-contract/fixtures/device-doctor.json`
// (locked by DeviceDoctorTest). The device writes `detail`; clients never
// compose it.

/** One reported check. [state] stays a raw string: an unknown one is kept, not dropped. */
data class DeviceDoctorItem(
    val key: String,
    val group: String,
    val parent: String? = null,
    val state: String,
    val detail: String? = null,
    val action: String? = null,
)

data class DeviceDoctor(
    val checkedAt: String? = null,
    val items: List<DeviceDoctorItem> = emptyList(),
)

private val doctorJson = Json { ignoreUnknownKeys = true }

private fun JsonObject.str(name: String): String? =
    (this[name] as? JsonPrimitive)?.takeIf { it.isString }?.content

/**
 * The stored `doctor` jsonb text → the model, LENIENTLY: null/bad = null (an
 * older build, which renders no block), and one malformed item (no key, no
 * group, no state) drops that item, never the whole blob.
 */
fun parseDeviceDoctor(raw: String?): DeviceDoctor? {
    if (raw.isNullOrBlank()) return null
    val obj = runCatching { doctorJson.parseToJsonElement(raw).jsonObject }.getOrNull()
        ?: return null
    val items = runCatching { obj["items"]?.jsonArray }.getOrNull().orEmpty().mapNotNull { el ->
        val item = el as? JsonObject ?: return@mapNotNull null
        DeviceDoctorItem(
            key = item.str("key") ?: return@mapNotNull null,
            group = item.str("group") ?: return@mapNotNull null,
            parent = item.str("parent"),
            state = item.str("state") ?: return@mapNotNull null,
            detail = item.str("detail"),
            action = item.str("action"),
        )
    }
    return DeviceDoctor(checkedAt = obj.str("checkedAt"), items = items)
}

/** The fixture's `glyph` vocabulary; [None] = the computer_use switch row. */
enum class DoctorGlyph { Check, Alert, Dash, X, None }

/** The fixture's `tone` vocabulary. */
enum class DoctorTone { Success, Warning, Muted, Destructive }

/** One rendered row of the block. */
data class DoctorRow(
    val key: String,
    val label: String,
    /** The device-written detail; always null on the switch row. */
    val detail: String?,
    val state: String,
    val glyph: DoctorGlyph,
    val tone: DoctorTone,
    /** A permission row under its parent (rendered indented). */
    val indented: Boolean,
    /** The `computer_use` item: label + switch, no glyph, no detail. */
    val isSwitch: Boolean = false,
    val switchOn: Boolean = false,
    /** The OFFERED action (null when none, or not offered remotely). */
    val action: String? = null,
    val actionLabel: String? = null,
    /** The ONE filled pill of the block. */
    val primary: Boolean = false,
)

data class DoctorGroup(
    val key: String,
    val label: String,
    /** `optional` on the agent and computer-use groups; null on Required. */
    val tag: String?,
    val rows: List<DoctorRow>,
)

object DeviceReadiness {

    const val GROUP_REQUIRED = "required"
    const val GROUP_AGENTS = "agents"
    const val GROUP_COMPUTER_USE = "computer_use"

    const val KEY_GIT = "git"
    const val KEY_COMPUTER_USE = "computer_use"

    const val STATE_OK = "ok"
    const val STATE_ACTION = "action"
    const val STATE_MISSING = "missing"
    const val STATE_OFF = "off"
    const val STATE_ERROR = "error"

    const val ACTION_INSTALL = "install"
    const val ACTION_UPDATE = "update"
    const val ACTION_SIGN_IN = "sign_in"
    const val ACTION_GRANT = "grant"

    /** Fixture `groups`, in block order. */
    val GROUPS: List<Triple<String, String, String?>> = listOf(
        Triple(GROUP_REQUIRED, "Required", null),
        Triple(GROUP_AGENTS, "Coding agents", "optional"),
        Triple(GROUP_COMPUTER_USE, "Computer use", "optional"),
    )

    /** Fixture `labels`. */
    val LABELS: Map<String, String> = mapOf(
        "git" to "Git",
        "claude" to "Claude Code",
        "codex" to "Codex",
        "computer_use" to "Computer use",
        "screen_recording" to "Screen Recording",
        "accessibility" to "Accessibility",
        "remote_desktop" to "Remote desktop",
    )

    /** Fixture `actions`: label + whether ANOTHER device may run it. */
    data class Action(val label: String, val remote: Boolean)

    val ACTIONS: Map<String, Action> = mapOf(
        ACTION_INSTALL to Action("Install", remote = false),
        ACTION_UPDATE to Action("Update", remote = true),
        ACTION_SIGN_IN to Action("Sign in", remote = true),
        ACTION_GRANT to Action("Open System Settings", remote = false),
    )

    fun label(key: String): String = LABELS[key] ?: key

    /** Fixture `states`; an unknown state reads as the muted dash. */
    fun glyphAndTone(state: String): Pair<DoctorGlyph, DoctorTone> = when (state) {
        STATE_OK -> DoctorGlyph.Check to DoctorTone.Success
        STATE_ACTION -> DoctorGlyph.Alert to DoctorTone.Warning
        STATE_ERROR -> DoctorGlyph.X to DoctorTone.Destructive
        else -> DoctorGlyph.Dash to DoctorTone.Muted
    }

    private fun needsAttention(state: String) = state == STATE_ACTION || state == STATE_ERROR

    /** An action is offered on the device itself always, elsewhere only when `remote`. */
    private fun offered(action: String?, remote: Boolean): String? {
        val spec = action?.let(ACTIONS::get) ?: return null
        return action.takeIf { !remote || spec.remote }
    }

    /**
     * The block's groups for [doctor]. [remote] = rendered for ANOTHER device
     * (a phone is always remote). [computerUseOn] overrides the switch row's
     * state with the LOCAL launch-defaults value (the switch writes
     * `launch_defaults.computerUse`; the doctor follows on the next heartbeat);
     * null = read it off the doctor (`off` = off, anything else = on).
     */
    fun groups(
        doctor: DeviceDoctor,
        remote: Boolean,
        computerUseOn: Boolean? = null,
    ): List<DoctorGroup> {
        val stateOf = doctor.items.associate { it.key to it.state }
        fun effectiveState(item: DeviceDoctorItem): String =
            if (item.key == KEY_COMPUTER_USE && computerUseOn == false) STATE_OFF else item.state

        val visible = doctor.items.filter { item ->
            val parent = item.parent ?: return@filter true
            val parentItem = doctor.items.firstOrNull { it.key == parent }
            val parentState = parentItem?.let(::effectiveState) ?: stateOf[parent]
            parentState != STATE_OFF
        }

        // Block order: the fixture's groups first, any group this build does
        // not know after them, each in first-seen order.
        val known = GROUPS.map { it.first }
        val extra = visible.map { it.group }.distinct().filter { it !in known }
        val ordered = GROUPS + extra.map { Triple(it, it, null) }

        var primaryTaken = false
        return ordered.mapNotNull { (groupKey, groupLabel, tag) ->
            val rows = visible.filter { it.group == groupKey }.map { item ->
                val state = effectiveState(item)
                if (item.key == KEY_COMPUTER_USE) {
                    DoctorRow(
                        key = item.key,
                        label = label(item.key),
                        detail = null,
                        state = state,
                        glyph = DoctorGlyph.None,
                        tone = DoctorTone.Muted,
                        indented = false,
                        isSwitch = true,
                        switchOn = computerUseOn ?: (state != STATE_OFF),
                    )
                } else {
                    val (glyph, tone) = glyphAndTone(state)
                    val action = offered(item.action, remote)
                    // The primary pill = the first action/error row's offered
                    // action, in block order.
                    val primary = !primaryTaken && action != null && needsAttention(state)
                    if (primary) primaryTaken = true
                    DoctorRow(
                        key = item.key,
                        label = label(item.key),
                        detail = item.detail,
                        state = state,
                        glyph = glyph,
                        tone = tone,
                        indented = item.parent != null,
                        action = action,
                        actionLabel = action?.let { ACTIONS[it]?.label },
                        primary = primary,
                    )
                }
            }
            rows.takeIf { it.isNotEmpty() }?.let { DoctorGroup(groupKey, groupLabel, tag, it) }
        }
    }

    /**
     * The agents the doctor says can run: agent items whose state is `ok`,
     * and none at all while a REQUIRED item (Git) is not ok.
     */
    fun runnableAgents(doctor: DeviceDoctor): List<String> {
        if (doctor.items.any { it.group == GROUP_REQUIRED && it.state != STATE_OK }) return emptyList()
        return doctor.items.filter { it.group == GROUP_AGENTS && it.state == STATE_OK }.map { it.key }
    }

    /**
     * The composer's single row: for the picked [agent] on this device, the
     * Git row when Git is the failure, else that agent's row when it cannot
     * run; null when nothing is wrong (or the doctor does not know the agent).
     * Same row, same action — and, being alone, its pill is the primary one.
     */
    fun failingRow(doctor: DeviceDoctor, agent: String, remote: Boolean = true): DoctorRow? {
        val git = doctor.items.firstOrNull { it.key == KEY_GIT }
        val item = git?.takeIf { it.state != STATE_OK }
            ?: doctor.items.firstOrNull { it.key == agent && it.group == GROUP_AGENTS }
                ?.takeIf { it.state != STATE_OK }
            ?: return null
        return singleRow(item, remote)
    }

    /**
     * The rows that need attention (`action`/`error`), each standalone — the
     * own-device list's caption, in block order. Switch rows never qualify,
     * nor optional `missing`/`off` ones (grey, never a problem).
     */
    fun attentionRows(doctor: DeviceDoctor, remote: Boolean = true): List<DoctorRow> =
        groups(doctor, remote).flatMap { it.rows }
            .filter { !it.isSwitch && needsAttention(it.state) }

    private fun singleRow(item: DeviceDoctorItem, remote: Boolean): DoctorRow {
        val (glyph, tone) = glyphAndTone(item.state)
        val action = offered(item.action, remote)
        return DoctorRow(
            key = item.key,
            label = label(item.key),
            detail = item.detail,
            state = item.state,
            glyph = glyph,
            tone = tone,
            indented = false,
            action = action,
            actionLabel = action?.let { ACTIONS[it]?.label },
            primary = action != null,
        )
    }
}
