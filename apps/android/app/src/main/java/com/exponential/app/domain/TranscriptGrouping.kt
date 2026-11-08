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
        const val MAX_WIDTH_WIDE = 0.62f
        const val RADIUS = 16
        const val NAME_SIZE = 12
    }

    object Copy {
        const val FORMER_MEMBER = "Former member"
        const val LINK_CHEVRON = "›"
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
        /** The session that started it (session-chat.json `origin`), else null. */
        val originSessionId: String?,
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
        var originPending = !chat.originSessionId.isNullOrEmpty()

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
                if (cross != null) {
                    side = Side.Other
                    sender = cross
                } else if (own) {
                    side = Side.Own
                } else {
                    side = Side.Other
                    val origin = if (originPending && item.authorSessionId.isNullOrEmpty()) {
                        fromSession(chat.originSessionId)
                    } else null
                    sender = when {
                        origin != null -> origin
                        item.authorKind == "user" -> {
                            val id = item.authorId ?: ""
                            Sender(SenderKind.Member, id, members[id] ?: Copy.FORMER_MEMBER, null)
                        }
                        else -> ownAgent
                    }
                }
                if (!own && item.authorSessionId.isNullOrEmpty()) originPending = false
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
                avatar = side == Side.Other && !sameGroup,
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
}
