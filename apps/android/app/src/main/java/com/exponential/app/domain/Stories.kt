package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/**
 * S-3 STORIES ×4 (`packages/domain-contract/fixtures/stories.json`, TS
 * reference `src/stories.ts`, iOS ExpCore `Stories.swift`, desktop
 * `domain::stories`): a session's own show cards (a plan, a question, a
 * result) folded into one ring per agent and per chat, for the story ring +
 * viewer. `StoriesTest` replays every fixture case, comparing the WHOLE
 * expected array.
 */
object Stories {

    enum class Kind(val wire: String) { Plan("plan"), Question("question"), Result("result") }

    enum class Footer(val wire: String) { Plan("plan"), Choices("choices"), Reply("reply"), None("none") }

    object Constants {
        const val SEEN_HOURS = 24L
        const val UNSEEN_HOURS = 168L
        const val AUTO_ADVANCE_MS = 6000L
    }

    object Copy {
        const val PLAN = "Plan"
        const val QUESTION = "Question"
        const val RESULT = "Result"
        const val NOW = "now"
        const val APPROVE = "Approve"
        const val CHANGE = "Change"
        const val REPLY = "Reply…"
        const val CLOSE = "Close"
        const val NEXT = "Next"
        const val PREVIOUS = "Previous"

        fun kind(kind: Kind): String = when (kind) {
            Kind.Plan -> PLAN
            Kind.Question -> QUESTION
            Kind.Result -> RESULT
        }
    }

    /** The ring's stroke (dp) and the gap between its arcs (degrees). */
    object Ring {
        const val WIDTH = 3f
        const val GAP = 6f
    }

    /** THE StoryProgress: segment height + gap (dp). */
    object Progress {
        const val HEIGHT = 3f
        const val GAP = 4f
    }

    /** The wide modal's max size + radius (dp; phones = full screen). */
    object Viewer {
        const val WIDTH = 440f
        const val HEIGHT = 760f
        const val RADIUS = 16f
    }

    object TestIds {
        const val RING = "story-ring"
        const val VIEWER = "story-viewer"
        const val PROGRESS = "story-progress"
        const val HEADER = "story-header"
        const val NEXT = "story-next"
        const val PREVIOUS = "story-previous"
    }

    // ── input ──────────────────────────────────────────────────────────────

    data class SessionInput(
        val id: String,
        val rootId: String,
        /** The session's display name (session-title). */
        val name: String,
        val agentId: String?,
        val startedByAgentId: String?,
    )

    data class AgentInput(val id: String, val name: String, val assistant: Boolean)

    data class MessageInput(
        val id: String,
        val sessionId: String,
        val authorSessionId: String?,
        /** Epoch ms. */
        val createdAt: Long,
        val answeredAt: Long?,
        /** The raw `messages.card`. */
        val card: JsonObject?,
    )

    data class ReadInput(
        /** The root session of the read mark. */
        val sessionId: String,
        val readAt: Long?,
        val storySeenAt: Long?,
    )

    // ── output ─────────────────────────────────────────────────────────────

    data class Picture(
        val label: String,
        val attachmentId: String,
        val width: Double?,
        val height: Double?,
        val caption: String?,
    )

    data class Story(
        val id: String,
        val sessionId: String,
        val rootId: String,
        val sessionName: String,
        val kind: Kind,
        val waiting: Boolean,
        val seen: Boolean,
        val createdAt: Long,
        val meta: String,
        val title: String?,
        val pictures: List<Picture>,
        val surface: Boolean,
        val text: String?,
        val footer: Footer,
        val autoAdvanceMs: Long?,
    )

    data class StoryRing(
        val key: String,
        val agentId: String?,
        val rootId: String?,
        val title: String,
        val assistant: Boolean,
        val waiting: Int,
        val unseen: Boolean,
        val stories: List<Story>,
    )

    // ── the fold ───────────────────────────────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun JsonObject.num(key: String): Double? =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull

    private fun JsonObject.isNull(key: String): Boolean = this[key] == null || this[key] is JsonNull

    private fun resolvedPictures(card: JsonObject): List<Picture> =
        (card["pictures"] as? JsonArray).orEmpty().mapNotNull { raw ->
            val picture = raw as? JsonObject ?: return@mapNotNull null
            val attachmentId = picture.str("attachmentId")?.takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            Picture(
                label = picture.str("label") ?: "",
                attachmentId = attachmentId,
                width = picture.num("width"),
                height = picture.num("height"),
                caption = picture.str("caption"),
            )
        }

    /** The card's story flag: explicit, else ask/plan or a resolved picture. */
    fun isStoryCard(card: JsonObject?): Boolean {
        if (card == null) return false
        val kind = card.str("kind")
        if (kind != "plan" && kind != "question" && kind != "surface") return false
        if (!card.str("questionId").isNullOrEmpty()) return false
        val story = (card["story"] as? JsonPrimitive)?.takeIf { !it.isString }?.content
        if (story == "true") return true
        if (story == "false") return false
        return kind != "surface" || resolvedPictures(card).isNotEmpty()
    }

    /** `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `now`. */
    fun age(createdAt: Long, now: Long): String {
        val minutes = (now - createdAt).coerceAtLeast(0) / 60_000
        if (minutes < 1) return Copy.NOW
        if (minutes < 60) return "${minutes}m"
        val hours = minutes / 60
        if (hours < 24) return "${hours}h"
        return "${hours / 24}d"
    }

    private const val HOUR = 3_600_000L

    private val byCreated = compareBy<Story>({ it.createdAt }, { it.id })

    private enum class Resolution { Approved, Changed, Answered, Superseded }

    private data class Built(val story: Story, val root: SessionInput, val resolution: Resolution?)

    private fun marksOf(reads: List<ReadInput>): Map<String, Long> = reads.associate { read ->
        read.sessionId to maxOf(read.readAt ?: Long.MIN_VALUE, read.storySeenAt ?: Long.MIN_VALUE)
    }

    /** One message as a story (null = not a story of a known root).
     *  `resolution` = how a plan/question was settled (null while waiting,
     *  and for results). */
    private fun storyOf(
        message: MessageInput,
        sessionsById: Map<String, SessionInput>,
        marks: Map<String, Long>,
        now: Long,
    ): Built? {
        val author = message.authorSessionId ?: return null
        if (author != message.sessionId) return null
        if (!isStoryCard(message.card)) return null
        val card = message.card ?: return null
        val session = sessionsById[message.sessionId] ?: return null
        val root = sessionsById[session.rootId] ?: return null
        val kind = when (card.str("kind")) {
            "plan" -> Kind.Plan
            "question" -> Kind.Question
            else -> Kind.Result
        }
        val waiting = when (kind) {
            Kind.Plan -> card.isNull("decision")
            Kind.Question -> message.answeredAt == null && card.isNull("answer")
            Kind.Result -> false
        }
        val resolution = when {
            kind == Kind.Result || waiting -> null
            kind == Kind.Question -> Resolution.Answered
            card.str("decision") == "approved" -> Resolution.Approved
            card.str("decision") == "superseded" -> Resolution.Superseded
            else -> Resolution.Changed
        }
        val pictures = resolvedPictures(card)
        val surface = card["surface"] is JsonObject
        val options = (card["options"] as? JsonArray).orEmpty()
        val title = when (kind) {
            Kind.Result -> card.str("title")?.trim()?.takeIf { it.isNotEmpty() }
            Kind.Question -> card.str("question")?.trim()?.takeIf { it.isNotEmpty() }
            Kind.Plan -> AskDock.askDockTitle(card.str("plan") ?: "").takeIf { it.isNotEmpty() }
        }
        val age = age(message.createdAt, now)
        val outcome = when (resolution) {
            Resolution.Approved -> Tab.Copy.APPROVED
            Resolution.Changed -> Tab.Copy.CHANGED
            Resolution.Answered -> Tab.Copy.ANSWERED
            Resolution.Superseded, null -> null
        }
        val story = Story(
            id = message.id,
            sessionId = session.id,
            rootId = root.id,
            sessionName = session.name,
            kind = kind,
            waiting = waiting,
            // A settled plan/question was acted on: seen.
            seen = resolution != null || message.createdAt <= (marks[root.id] ?: Long.MIN_VALUE),
            createdAt = message.createdAt,
            meta = if (outcome != null) "${Copy.kind(kind)} · $outcome · $age" else "${Copy.kind(kind)} · $age",
            title = title,
            pictures = pictures,
            surface = surface,
            text = if (kind == Kind.Plan && pictures.isEmpty() && !surface) card.str("plan") else null,
            footer = when {
                resolution != null -> Footer.None
                kind == Kind.Plan -> Footer.Plan
                kind == Kind.Question -> if (options.isNotEmpty()) Footer.Choices else Footer.Reply
                else -> Footer.None
            },
            autoAdvanceMs = if (kind == Kind.Result) Constants.AUTO_ADVANCE_MS else null,
        )
        return Built(story, root, resolution)
    }

    /** The ring a root's stories ride (an agent's, else the chat's), empty. */
    private fun emptyRing(root: SessionInput, agentsById: Map<String, AgentInput>): StoryRing {
        val agentId = root.agentId ?: root.startedByAgentId
        val agent = agentId?.let { agentsById[it] }
        return StoryRing(
            key = if (agentId != null) "agent:$agentId" else "root:${root.id}",
            agentId = agentId,
            rootId = if (agentId == null) root.id else null,
            title = if (agentId != null) agent?.name ?: root.name else root.name,
            assistant = agent?.assistant ?: false,
            waiting = 0,
            unseen = false,
            stories = emptyList(),
        )
    }

    /** THE fold: the viewer's story rings, in display order. */
    fun rings(
        now: Long,
        sessions: List<SessionInput>,
        agents: List<AgentInput>,
        messages: List<MessageInput>,
        reads: List<ReadInput>,
    ): List<StoryRing> {
        val sessionsById = sessions.associateBy { it.id }
        val agentsById = agents.associateBy { it.id }
        val marks = marksOf(reads)
        data class Draft(val head: StoryRing, val stories: MutableList<Story>)
        val drafts = LinkedHashMap<String, Draft>()

        for (message in messages) {
            val built = storyOf(message, sessionsById, marks, now) ?: continue
            // A settled plan/question leaves the ring (the Stories tab keeps it).
            if (built.resolution != null) continue
            val story = built.story
            if (story.kind == Kind.Result) {
                val keep = if (story.seen) Constants.SEEN_HOURS else Constants.UNSEEN_HOURS
                if (now - story.createdAt >= keep * HOUR) continue
            }
            val head = emptyRing(built.root, agentsById)
            drafts.getOrPut(head.key) { Draft(head, mutableListOf()) }.stories.add(story)
        }

        val list = drafts.values.map { draft ->
            val waiting = draft.stories.filter { it.waiting }.sortedWith(byCreated)
            val unseen = draft.stories.filter { !it.waiting && !it.seen }.sortedWith(byCreated)
            val seen = draft.stories.filter { !it.waiting && it.seen }.sortedWith(byCreated)
            val stories = waiting + unseen + seen
            draft.head.copy(
                stories = stories,
                waiting = waiting.size,
                unseen = stories.any { it.waiting || !it.seen },
            )
        }
        fun rank(ring: StoryRing) = when {
            ring.assistant -> 0
            ring.waiting > 0 -> 1
            ring.unseen -> 2
            else -> 3
        }
        fun newest(ring: StoryRing) = ring.stories.maxOf { it.createdAt }
        return list.sortedWith(
            compareBy<StoryRing>({ rank(it) }, { -newest(it) }, { it.key }),
        )
    }

    /** THE Stories tab of a session screen (stories.json `tab`): every story
     *  the root's tree ever posted, never expiring; settled plans/questions
     *  stay (footer none, `meta` names the outcome), a superseded plan drops.
     *  Waiting first (oldest first), then the rest newest first. ONE ring
     *  (the root's, as [rings] keys it) so the viewer opens on it; null = the
     *  empty state. `unseen` = the tab's dot. */
    fun storyTab(
        now: Long,
        sessions: List<SessionInput>,
        agents: List<AgentInput>,
        messages: List<MessageInput>,
        reads: List<ReadInput>,
        rootId: String,
    ): StoryRing? {
        val sessionsById = sessions.associateBy { it.id }
        val agentsById = agents.associateBy { it.id }
        val root = sessionsById[rootId] ?: return null
        if (root.rootId != root.id) return null
        val marks = marksOf(reads)
        val list = messages.mapNotNull { message ->
            storyOf(message, sessionsById, marks, now)
                ?.takeIf { it.root.id == root.id && it.resolution != Resolution.Superseded }
                ?.story
        }
        if (list.isEmpty()) return null
        val waiting = list.filter { it.waiting }.sortedWith(byCreated)
        val rest = list.filter { !it.waiting }.sortedWith(byCreated.reversed())
        return emptyRing(root, agentsById).copy(
            waiting = waiting.size,
            unseen = list.any { it.waiting || !it.seen },
            stories = waiting + rest,
        )
    }

    /** THE Stories tab (stories.json `tab`): copy, testIds, tile geometry. */
    object Tab {
        object Copy {
            const val LABEL = "Stories"
            const val EMPTY = "No stories yet"
            const val APPROVED = "Approved"
            const val CHANGED = "Changes asked"
            const val ANSWERED = "Answered"
            const val SUPERSEDED = "Replaced"
            const val CHANGES = "Changes"
            const val STACK = "Stack"
        }

        object TestIds {
            const val TAB = "thread-stories-tab"
            const val LIST = "thread-stories"
            const val TILE = "story-tile"
            const val CHANGES = "thread-stories-changes"
            const val STACK = "thread-stories-stack"
        }

        /** Tile geometry (dp); Android = the `phone` form factor. */
        object Tile {
            const val THUMB_HEIGHT = 140f
            const val THUMB_MAX_WIDTH = 280f
            const val GAP = 8f
            const val RADIUS = 10f
            const val LIST_GAP = 16f
            const val LEAD_GAP = 24f
        }

        /** `tab.specimen.case`: the styleguide specimen draws this tabCase. */
        const val SPECIMEN_CASE = 1
    }

    /** The chats header row (chats.json BUBBLES + STORY RING): each agent
     *  bubble wears its agent's ring ([BubbleRings.byAgent], first wins);
     *  [BubbleRings.rest] = the rings no bubble wears (the root rings, an
     *  agent ring with no bubble), in ring order, painted after the bubbles
     *  in the same row. */
    data class BubbleRings(val byAgent: Map<String, StoryRing>, val rest: List<StoryRing>)

    fun bubbleRings(bubbleAgentIds: List<String>, rings: List<StoryRing>): BubbleRings {
        val ids = bubbleAgentIds.toSet()
        val byAgent = LinkedHashMap<String, StoryRing>()
        val rest = mutableListOf<StoryRing>()
        for (ring in rings) {
            val agentId = ring.agentId
            if (agentId != null && agentId in ids && agentId !in byAgent) byAgent[agentId] = ring else rest += ring
        }
        return BubbleRings(byAgent, rest)
    }
}
