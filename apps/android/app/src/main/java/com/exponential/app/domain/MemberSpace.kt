package com.exponential.app.domain

import com.exponential.app.data.db.DeviceEntity

/**
 * #EXP-2161 (packages/domain-contract/fixtures/member-space.json, THE spec;
 * TS reference `member-space.ts`): a member's private Assistant on a device
 * shared with their team. The pure rules the UI reads, replayed by
 * `MemberSpaceTest` (`optInCases`, `teamCases`, `deviceCases`, `startCases`,
 * `sectionCases`); the key rules (`pinCases`, `rootMarkCases`) live with the
 * install trust (`InstallTrust.kt`).
 */
object MemberSpace {
    const val DOCTOR_KEY = "member_space"
    const val DEVICE_ID_PREFIX = "space-"

    /** The fixture's `copy`, byte-identical ×4. */
    object Copy {
        const val OPT_IN = "Members' Assistants"
        const val OPT_IN_TITLE = "Let members run their Assistant here?"
        const val OPT_IN_BODY = "Your machine holds these members' chat keys and they can run code on it with the access of its logins. You could read their Assistant chats."
        const val OPT_IN_CONFIRM = "Allow"
        const val PICK_TITLE = "Run your Assistant on {device}?"
        const val PICK_BODY = "{owner} and admins of this machine can read what your Assistant does here and act as it in teams that allow it. It works only there, and a new conversation starts."
        const val PICK_CONFIRM = "Run here"
        const val MEMBERS = "Members"
        const val REMOVE = "Remove"
        const val WAITING = "Waiting for approval"
        const val NO_DEVICE = "Pick a device"
        const val ENDED = "Your Assistant no longer runs on {device}. Pick a device."
    }

    /** The opt-in confirm (`copy.optIn*`): Cancel · Allow (primary). */
    fun optInPrompt(): Prompts.Prompt = confirm(Copy.OPT_IN_TITLE, Copy.OPT_IN_BODY, ACTION_ALLOW, Copy.OPT_IN_CONFIRM)

    /** The pick confirm (`copy.pick*`): Cancel · Run here (primary). */
    fun pickPrompt(device: String, owner: String): Prompts.Prompt = confirm(
        Copy.PICK_TITLE.replace("{device}", device),
        Copy.PICK_BODY.replace("{owner}", owner),
        ACTION_PICK,
        Copy.PICK_CONFIRM,
    )

    const val ACTION_ALLOW = "allow"
    const val ACTION_PICK = "pick"

    private fun confirm(title: String, body: String, id: String, label: String) = Prompts.Prompt(
        title = title,
        body = body,
        actions = listOf(
            Prompts.Action("cancel", "Cancel", Prompts.Role.Cancel),
            Prompts.Action(id, label, Prompts.Role.Primary),
        ),
        focus = id,
    )

    /** `listed`: a member space (`space_host_id`) is never a device: every
     *  list and picker drops it; only what RESOLVES a bound id reads it. */
    fun isSpace(row: DeviceEntity): Boolean = !row.spaceHostId.isNullOrEmpty()

    /** `optInCases`: who may move one team in or out of a device's opt-in. */
    fun mayOptIn(caller: String, allowed: Boolean, kind: String, shared: Boolean, doctor: String?): Boolean {
        if (!allowed) return caller == "deviceOwner" || caller == "teamOwner"
        return caller == "deviceOwner" && kind == "server" && shared && doctor == "ok"
    }

    /** `teamCases`: the opted-in teams the host's owner and the member both
     *  belong to, in the member's order. */
    fun spaceTeams(assistantTeamIds: List<String>, hostOwnerTeamIds: List<String>, memberTeamIds: List<String>): List<String> =
        memberTeamIds.filter { it in assistantTeamIds && it in hostOwnerTeamIds }

    /** `deviceCases`: what a request speaking through a space may run on. */
    fun spaceMayUseDevice(device: String, ownTree: Boolean): Boolean =
        if (device == "space") ownTree else device == "otherShared"

    /** `startCases`: a space runs only the trees of its Assistant conversation. */
    fun maySpaceRun(tree: String, rootDeviceIsSpace: Boolean): Boolean = tree == "assistant" && rootDeviceIsSpace

    /** The device settings' opt-in row (`optInCases`): off is always
     *  allowed; [detail] = why it cannot (its doctor item's, else
     *  [DeviceReadiness.UNAVAILABLE]). */
    data class OptInRow(val disabled: Boolean, val detail: String?)

    fun optInRow(checked: Boolean, kind: String, doctor: DeviceDoctor?): OptInRow {
        val item = doctor?.items?.firstOrNull { it.key == DOCTOR_KEY }
        val able = mayOptIn("deviceOwner", allowed = true, kind = kind, shared = true, doctor = item?.state)
        return OptInRow(
            disabled = !able && !checked,
            detail = if (able) null else item?.detail?.takeIf { it.isNotEmpty() } ?: DeviceReadiness.UNAVAILABLE,
        )
    }

    data class Device(
        val rowId: String,
        val deviceId: String,
        val userId: String,
        val label: String,
        val online: Boolean,
        val assistantTeamIds: List<String>,
        val spaceHostId: String?,
    )

    data class Team(val id: String, val name: String)
    data class SectionRow(val id: String, val label: String, val disabled: Boolean, val selected: Boolean)
    data class Section(val teamId: String, val title: String, val rows: List<SectionRow>)

    /** `sectionCases`: Runs on's team sections, other people's opted-in
     *  hosts per team in the member's order, label order inside; own hosts
     *  and spaces never. The bound space shows as its host's row. */
    fun spaceSections(devices: List<Device>, teams: List<Team>, boundId: String?, me: String): List<Section> {
        val boundHost = devices.firstOrNull { it.deviceId == boundId }?.spaceHostId
        return teams.mapNotNull { team ->
            val rows = devices
                .filter { it.userId != me && it.spaceHostId.isNullOrEmpty() && team.id in it.assistantTeamIds }
                .sortedWith { a, b -> a.label.compareTo(b.label) }
                .map { device ->
                    val selected = boundHost != null && device.rowId == boundHost
                    SectionRow(device.deviceId, device.label, disabled = !device.online && !selected, selected = selected)
                }
            if (rows.isEmpty()) null else Section(team.id, team.name, rows)
        }
    }

    /** One team device's picker row (web `SpacePick`); [value] names the team
     *  too, since one host may be listed under several. */
    data class Pick(
        val value: String,
        val name: String,
        val icon: String?,
        val section: String,
        val teamId: String,
        val hostDeviceId: String,
        val ownerId: String,
        val selected: Boolean,
        val disabled: Boolean,
        val online: Boolean,
    )

    data class Picks(val picks: List<Pick>, val value: String?)

    /** The picker rows of [spaceSections], in order, and the picked one's
     *  value (the bound space's own team first). */
    fun picks(rows: List<DeviceEntity>, teams: List<Team>, boundId: String?, me: String, nowMs: Long): Picks {
        val sections = spaceSections(
            rows.map {
                Device(
                    rowId = it.id,
                    deviceId = it.deviceId,
                    userId = it.userId,
                    label = it.label,
                    online = DeviceLiveness.isOnline(it.lastSeenAt, nowMs),
                    assistantTeamIds = it.assistantTeamIds,
                    spaceHostId = it.spaceHostId,
                )
            },
            teams,
            boundId,
            me,
        )
        val picks = sections.flatMap { section ->
            section.rows.map { row ->
                val host = rows.firstOrNull { it.deviceId == row.id && it.userId != me && section.teamId in it.assistantTeamIds }
                Pick(
                    value = "${section.teamId}:${row.id}",
                    name = row.label.ifEmpty { row.id },
                    icon = host?.icon,
                    section = section.title,
                    teamId = section.teamId,
                    hostDeviceId = row.id,
                    ownerId = host?.userId.orEmpty(),
                    selected = row.selected,
                    disabled = row.disabled,
                    online = host?.let { DeviceLiveness.isOnline(it.lastSeenAt, nowMs) } == true,
                )
            }
        }
        val space = rows.firstOrNull { it.deviceId == boundId && isSpace(it) }
        val picked = picks.filter { it.selected }
        val value = (picked.firstOrNull { it.teamId == space?.spaceTeamId } ?: picked.firstOrNull())?.value
        return Picks(picks, value)
    }
}

/** `listed`: the rows every device list and picker reads (no member space). */
fun List<DeviceEntity>.listed(): List<DeviceEntity> = filterNot(MemberSpace::isSpace)
