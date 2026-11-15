package com.exponential.app.domain

// #EXP-1736: ONE transcript ×4, every chat a group chat. Who sent each row,
// which rows share an avatar, the gaps between them.
// fixtures/transcript-grouping.json holds the rules + cases; mirrors: TS
// `transcript-grouping.ts`, desktop `domain::transcript_grouping`, iOS ExpCore
// `TranscriptGrouping.swift`.
object TranscriptGrouping {

    /** `layout` of the fixture (dp / sp / fractions). */
    object Layout {
        const val AVATAR = 32
        const val AVATAR_GAP = 8
        const val GAP_IN_GROUP = 3
        const val GAP_BETWEEN = 12
        const val MAX_WIDTH_PHONE = 0.78f
        const val MAX_WIDTH_PHONE_IN = 1f

        /** #FEED-239: an own bubble may take the whole column left of the
         *  viewer's avatar, on every width. */
        const val MAX_WIDTH_OWN = 1f
        const val MAX_WIDTH_WIDE = 0.62f
        const val RADIUS = 16
        const val NAME_SIZE = 12
    }

    object Copy {
        const val FORMER_MEMBER = "Former member"
        const val LINK_CHEVRON = "›"
        const val SHOW_MORE = "Show more"
        const val SHOW_LESS = "Show less"

        /** #EXP-1772 `copy.markers`: a system input's marker label by key. */
        val MARKERS: Map<String, String> = linkedMapOf(
            "schedule" to "Schedule",
            "event" to "Event",
            "pr_opened" to "Pull request opened",
            "pr_merged" to "Pull request merged",
            "pr_merge" to "Merge",
            "pr_checks" to "CI",
            "session" to "Session update",
        )

        /** #EXP-1873 `copy.checkIn*`: the quiet check-in line. */
        const val CHECK_IN = "Checked in, nothing to do"
        const val CHECK_INS = "Checked in {count} times, nothing to do"
        const val CHECK_IN_TIME_SEPARATOR = " · "
    }

    enum class ItemKind(val wire: String) { Message("message"), Trace("trace"), Full("full") }

    data class Item(
        val id: String,
        val kind: ItemKind,
        /** [ItemKind.Message] only: `user` | `agent` | `system`. */
        val authorKind: String? = null,
        val authorId: String? = null,
        val authorSessionId: String? = null,
    )

    data class Chat(
        /** The session whose conversation this is. */
        val sessionId: String,
        val title: String,
        val agentId: String?,
        /** The session that started it (session-chat.json `origin`, from the
         *  synced rows), else null. */
        val originSessionId: String?,
        /** The items begin at the conversation's first message (fixture absent = true). */
        val fromStart: Boolean = true,
    )

    data class Named(val id: String, val name: String)

    data class SessionRef(val id: String, val title: String, val agentId: String?)

    data class Input(
        val viewerId: String?,
        val chat: Chat,
        val members: List<Named> = emptyList(),
        val agents: List<Named> = emptyList(),
        val sessions: List<SessionRef> = emptyList(),
        val items: List<Item>,
    )

    enum class SenderKind(val wire: String) { Member("member"), Agent("agent"), Session("session") }

    data class Sender(
        val kind: SenderKind,
        val id: String,
        val name: String,
        /** A cross-session sender: the session a press opens. */
        val link: String?,
    ) {
        /** The bubble's name line: a cross-session name ends with ` ›`. */
        val label: String get() = if (link != null) "$name ${Copy.LINK_CHEVRON}" else name
    }

    enum class Side(val wire: String) { Own("own"), Other("other"), Full("full") }

    data class RowLayout(
        val id: String,
        val side: Side,
        val sender: Sender?,
        val avatar: Boolean,
        val name: Boolean,
        val gap: Int,
    )

    fun transcriptLayout(input: Input): List<RowLayout> {
        val chat = input.chat
        val sessions = input.sessions.associateBy { it.id }
        val members = input.members.associate { it.id to it.name }
        val agents = input.agents.associate { it.id to it.name }
        fun fromSession(id: String?): Sender? {
            val session = if (!id.isNullOrEmpty() && id != chat.sessionId) sessions[id] else null
            session ?: return null
            return if (!session.agentId.isNullOrEmpty()) {
                Sender(SenderKind.Agent, session.agentId, session.title, session.id)
            } else {
                Sender(SenderKind.Session, session.id, session.title, session.id)
            }
        }
        val ownAgent = if (!chat.agentId.isNullOrEmpty()) {
            Sender(SenderKind.Agent, chat.agentId, agents[chat.agentId] ?: chat.title, null)
        } else {
            Sender(SenderKind.Session, chat.sessionId, chat.title, null)
        }
        var originPending = !chat.originSessionId.isNullOrEmpty() && chat.fromStart

        val out = ArrayList<RowLayout>(input.items.size)
        var prevKey: String? = null
        var groupNamed = false
        for (item in input.items) {
            val side: Side
            var sender: Sender? = null
            if (item.kind == ItemKind.Full || (item.kind == ItemKind.Message && item.authorKind == "system")) {
                side = Side.Full
            } else if (item.kind == ItemKind.Trace) {
                side = Side.Other
                sender = ownAgent
            } else {
                val own = item.authorKind == "user" && !input.viewerId.isNullOrEmpty() &&
                    item.authorId == input.viewerId
                val cross = fromSession(item.authorSessionId)
                // The origin: the conversation's first INPUT (a person's row or
                // a known session's), unless it names its own sender; the
                // agent's own output never takes it.
                val isInput = item.authorKind == "user" || cross != null
                val origin = if (originPending && isInput && cross == null) fromSession(chat.originSessionId) else null
                if (isInput) originPending = false
                if (cross != null) {
                    side = Side.Other
                    sender = cross
                } else if (origin != null) {
                    side = Side.Other
                    sender = origin
                } else if (own) {
                    side = Side.Own
                } else {
                    side = Side.Other
                    sender = if (item.authorKind == "user") {
                        val id = item.authorId ?: ""
                        Sender(SenderKind.Member, id, members[id] ?: Copy.FORMER_MEMBER, null)
                    } else {
                        ownAgent
                    }
                }
            }
            val key = when (side) {
                Side.Full -> null
                Side.Own -> "own"
                Side.Other -> "${sender!!.kind.wire}:${sender.id}:${sender.link ?: ""}"
            }
            val sameGroup = key != null && key == prevKey
            if (!sameGroup) groupNamed = false
            val name = side == Side.Other && item.kind == ItemKind.Message && !groupNamed
            if (name) groupNamed = true
            out += RowLayout(
                id = item.id,
                side = side,
                sender = sender,
                // #FEED-239: the viewer's own group wears THEIR avatar on the right.
                avatar = side != Side.Full && !sameGroup,
                name = name,
                gap = if (out.isEmpty()) 0 else if (sameGroup) Layout.GAP_IN_GROUP else Layout.GAP_BETWEEN,
            )
            prevKey = key
        }
        return out
    }

    enum class InputKind(val wire: String) { Report("report"), Message("message") }

    data class CrossSessionInput(
        val kind: InputKind,
        val name: String,
        val sessionId: String,
        val body: String,
    )

    private val INPUT_PATTERN =
        Regex("""^(Report|Message) from (.+?) \(session ([^,)\s]+)(, ended)?\): ([\s\S]*)$""")

    /** A session input as the device words it (`coding::pickup::input_text`). */
    fun crossSessionInput(text: String): CrossSessionInput? {
        val match = INPUT_PATTERN.matchEntire(text) ?: return null
        val g = match.groupValues
        return CrossSessionInput(
            kind = if (g[1] == "Report") InputKind.Report else InputKind.Message,
            name = g[2],
            sessionId = g[3],
            body = g[5],
        )
    }

    // #EXP-1772: only real messages are bubbles. A system input (a schedule
    // or event firing, a PR or session status line) is THE inline marker,
    // drawn only above the agent's reply to it.

    sealed interface FeedInput {
        data class Marker(val key: String, val label: String) : FeedInput
        data class Relay(
            val name: String,
            val sessionId: String,
            val body: String,
            val report: Boolean,
        ) : FeedInput
    }

    private val TAG_PATTERN = Regex("""^\[([a-z_]+)\] """)
    private val SCHEDULE_PATTERN = Regex("""^Schedule `[^`\n]+`[^\n]* fired""")
    private val EVENT_PATTERN = Regex("""^Event `[^`\n]+` \(from """)

    private fun marker(key: String): FeedInput = FeedInput.Marker(key, Copy.MARKERS.getValue(key))

    /** A feed input: the device's tagged system line = a marker, another
     *  session's input = a relay, else null (a person's prompt). */
    fun feedInput(text: String): FeedInput? {
        val tag = TAG_PATTERN.find(text)?.groupValues?.get(1)
        if (tag != null && Copy.MARKERS.containsKey(tag)) return marker(tag)
        if (SCHEDULE_PATTERN.containsMatchIn(text)) return marker("schedule")
        if (EVENT_PATTERN.containsMatchIn(text)) return marker("event")
        val cross = crossSessionInput(text) ?: return null
        return FeedInput.Relay(cross.name, cross.sessionId, cross.body, cross.kind == InputKind.Report)
    }

    enum class FeedRowKind(val wire: String) { Marker("marker"), Input("input"), Reply("reply"), Other("other") }

    /** Per row: is it drawn? A marker only when a reply follows it before the
     *  next input or marker; every other row always. */
    fun markersShown(kinds: List<FeedRowKind>): List<Boolean> {
        val shown = BooleanArray(kinds.size) { true }
        var pending: Int? = null
        kinds.forEachIndexed { index, kind ->
            when (kind) {
                FeedRowKind.Marker, FeedRowKind.Input -> {
                    pending?.let { shown[it] = false }
                    pending = if (kind == FeedRowKind.Marker) index else null
                }
                FeedRowKind.Reply -> pending = null
                FeedRowKind.Other -> Unit
            }
        }
        pending?.let { shown[it] = false }
        return shown.toList()
    }

    /** #EXP-1873: a feed row as [feedRows] reads it: `schedule` = a schedule
     *  firing (a check-in), `marker` = every other marker, `quiet` = the quiet
     *  notice, `blank` = an agent turn with blank text. */
    enum class FeedRowRole(val wire: String) {
        Schedule("schedule"), Marker("marker"), Input("input"), Reply("reply"),
        Quiet("quiet"), Blank("blank"), Other("other"),
    }

    /** Per row: not drawn, drawn as itself, or THE check-in line covering [count] quiet check-ins. */
    sealed interface FeedRowShown {
        data object Hidden : FeedRowShown
        data object Row : FeedRowShown
        data class CheckIn(val count: Int) : FeedRowShown
    }

    /** #EXP-1873 (`checkInCases`): a quiet check-in (a schedule firing whose
     *  turn only called quiet, the forced filler after it aside) is one line;
     *  consecutive ones collapse onto the last. */
    fun feedRows(kinds: List<FeedRowRole>): List<FeedRowShown> {
        val drawn = markersShown(
            kinds.map {
                when (it) {
                    FeedRowRole.Schedule, FeedRowRole.Marker -> FeedRowKind.Marker
                    FeedRowRole.Input -> FeedRowKind.Input
                    FeedRowRole.Reply -> FeedRowKind.Reply
                    FeedRowRole.Quiet, FeedRowRole.Blank, FeedRowRole.Other -> FeedRowKind.Other
                }
            },
        )
        val shown = MutableList<FeedRowShown>(kinds.size) { index ->
            if (drawn[index] && kinds[index] != FeedRowRole.Blank) FeedRowShown.Row else FeedRowShown.Hidden
        }
        val ends = setOf(FeedRowRole.Schedule, FeedRowRole.Marker, FeedRowRole.Input)
        kinds.forEachIndexed { start, kind ->
            if (kind != FeedRowRole.Schedule) return@forEachIndexed
            var end = start + 1
            while (end < kinds.size && kinds[end] !in ends) end += 1
            var quiet = false
            for (index in start + 1 until end) {
                val row = kinds[index]
                if (row == FeedRowRole.Other || (row == FeedRowRole.Reply && !quiet)) return@forEachIndexed
                if (row == FeedRowRole.Quiet) quiet = true
            }
            if (!quiet) return@forEachIndexed
            shown[start] = FeedRowShown.CheckIn(1)
            for (index in start + 1 until end) shown[index] = FeedRowShown.Hidden
        }
        var line: Int? = null
        shown.forEachIndexed { index, row ->
            when (row) {
                is FeedRowShown.CheckIn -> {
                    line?.let {
                        shown[index] = FeedRowShown.CheckIn((shown[it] as FeedRowShown.CheckIn).count + row.count)
                        shown[it] = FeedRowShown.Hidden
                    }
                    line = index
                }
                FeedRowShown.Row -> line = null
                FeedRowShown.Hidden -> Unit
            }
        }
        return shown
    }

    /** #EXP-1873 (`checkInLabelCases`): the check-in line's text. */
    fun checkInLabel(count: Int, time: String?): String {
        val label = if (count == 1) Copy.CHECK_IN else Copy.CHECK_INS.replace("{count}", count.toString())
        return if (time.isNullOrEmpty()) label else "$label${Copy.CHECK_IN_TIME_SEPARATOR}$time"
    }

    /** #EXP-1873 `checkInSpecimen`: an agent chat's feed row (kind, `HH:mm`, text). */
    data class CheckInSpecimenRow(val kind: FeedRowRole, val time: String?, val text: String?)

    /** #EXP-1873: the chat-transcript specimen's agent chat: a run of quiet
     *  check-ins, one that answered, a second run. */
    val CHECK_IN_SPECIMEN: List<CheckInSpecimenRow> = listOf(
        CheckInSpecimenRow(FeedRowRole.Schedule, "16:30", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Schedule, "16:45", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Reply, "16:45", "No change since my last update."),
        CheckInSpecimenRow(FeedRowRole.Schedule, "17:00", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Schedule, "17:15", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Schedule, "17:30", null),
        CheckInSpecimenRow(FeedRowRole.Reply, "17:31", "Two PRs landed overnight: #412 and #415, both green."),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Schedule, "17:45", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
        CheckInSpecimenRow(FeedRowRole.Schedule, "18:00", null),
        CheckInSpecimenRow(FeedRowRole.Quiet, null, null),
    )

    /** #FEED-238 `fold`: a bubble over [CHARS] characters or [LINES] lines folds. */
    object Fold {
        const val LINES = 6
        const val CHARS = 600
    }

    /** #FEED-238 THE FOLD ×4 (`foldCases`): a message bubble longer than
     *  `fold.chars` characters or `fold.lines` lines folds behind Show more. */
    fun messageFolds(text: String): Boolean =
        text.length > Fold.CHARS || text.split("\n").size > Fold.LINES

    /** #EXP-1864 `notices`: an Exponential tool row drawn as THE inline marker
     *  (its own line, never a trace beside the avatar), by contract row. */
    val NOTICES: Map<String, String> = linkedMapOf(
        "session_self_quiet" to "Ended turn quietly",
    )

    /** #EXP-1864 (`noticeCases`): the marker label a tool call draws instead
     *  of its trace line, else null. The name must END in the contract prefix
     *  + the row, whatever namespace sits before it ([ExpToolDisplay.row]'s match). */
    fun traceNotice(toolName: String?): String? {
        val name = toolName?.trim().orEmpty()
        for ((row, label) in NOTICES) {
            if (name.length > row.length && name.endsWith(row) &&
                name.dropLast(row.length).endsWith(DomainContract.expToolPrefix)
            ) {
                return label
            }
        }
        return null
    }
}
