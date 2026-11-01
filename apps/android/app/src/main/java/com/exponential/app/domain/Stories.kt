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

    /** `open` = a plan: THE plan page (plan-page.json), never a decision in a story. */
    enum class Footer(val wire: String) { Open("open"), Choices("choices"), Reply("reply"), None("none") }

    object Constants {
        /** #EXP-1461: a ring holds a story while its age is under this. */
        const val WINDOW_HOURS = 24L
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
        /** Between the segments (#EXP-1493: one per story). */
        const val GAP = 4f
        /** Retired (#EXP-1493); the fixture keeps it for old mirrors. */
        const val STORY_GAP = 10f
    }

    /** #EXP-1493 THE kind badge ×4 (THE Pill, sm, readonly): the concept icon
     *  + the kind word, text in the tint, the tint at [FILL] behind, at
     *  [STROKE] as its border. */
    object Badge {
        fun icon(kind: Kind): String = when (kind) {
            Kind.Plan -> "coding-plan"
            Kind.Question -> "ui-help"
            Kind.Result -> "ui-check"
        }

        /** The ask-dock tints: plan primary, question yellow, result emerald. */
        fun tint(kind: Kind): String = when (kind) {
            Kind.Plan -> "primary"
            Kind.Question -> "yellow"
            Kind.Result -> "emerald"
        }

        const val FILL = 0.14f
        const val STROKE = 0.3f
    }

    /** The wide modal's max size + radius (dp; phones = full screen). */
    object Viewer {
        const val WIDTH = 440f
        const val HEIGHT = 760f
        const val RADIUS = 16f
        /** The floating band's scrim: at least this tall (dp) ... */
        const val SCRIM_HEIGHT = 180f
        /** ... fading to clear over the last this many dp past it. */
        const val SCRIM_FADE = 48f
        /** #EXP-1463: the wide modal's room to the viewport (x each side, y top + bottom). */
        const val INSET_X = 96f
        const val INSET_Y = 48f
        /** ... and its least width (a tall picture letterboxed on black). */
        const val MIN_WIDTH = 360f
    }

    /** THE STORY ROW of the chats header (#EXP-1461, `row`, dp). */
    object Row {
        const val ITEM_WIDTH = 72f
        const val GAP = 12f
        const val LABEL_GAP = 6f
        const val PAD_BOTTOM = 4f
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
        /** The card's `description` (a result's one line under its body). */
        val description: String? = null,
        val pictures: List<Picture>,
        val surface: Boolean,
        /** A plan's counts (plan-page `planCounts`), else null. */
        val counts: PlanPage.Counts?,
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

    /** #EXP-1493: newest first (createdAt desc, then id desc). */
    private val byNewest = compareByDescending<Story> { it.createdAt }.thenByDescending { it.id }

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
            description = card.str("description")?.trim()?.takeIf { it.isNotEmpty() },
            pictures = pictures,
            surface = surface,
            counts = if (kind == Kind.Plan) PlanPage.counts(card) else null,
            footer = when {
                kind == Kind.Plan -> Footer.Open
                resolution != null -> Footer.None
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
            // #EXP-1461: a ring holds the last `windowHours` only, waiting or not.
            if (now - story.createdAt >= Constants.WINDOW_HOURS * HOUR) continue
            val head = emptyRing(built.root, agentsById)
            drafts.getOrPut(head.key) { Draft(head, mutableListOf()) }.stories.add(story)
        }

        val list = drafts.values.map { draft ->
            // #EXP-1493: a session's newer result replaces its older ones in a ring.
            val newestResult = draft.stories.filter { it.kind == Kind.Result }
                .groupBy { it.sessionId }
                .mapValues { (_, results) -> results.sortedWith(byNewest).first().id }
            val kept = draft.stories.filter { it.kind != Kind.Result || newestResult[it.sessionId] == it.id }
            val waiting = kept.filter { it.waiting }.sortedWith(byNewest)
            val stories = waiting + kept.filter { !it.waiting }.sortedWith(byNewest)
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
     *  the root's tree ever posted, never expiring (no window cut); settled plans/questions
     *  stay (footer none, a plan keeps `open`; `meta` names the outcome), a
     *  superseded plan drops. Waiting first, then the rest, each newest first. ONE ring
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
        val waiting = list.filter { it.waiting }.sortedWith(byNewest)
        val rest = list.filter { !it.waiting }.sortedWith(byNewest)
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
            const val THUMB_HEIGHT = 240f
            const val THUMB_MAX_WIDTH = 320f
            const val GAP = 8f
            /** Line 1 → the description. */
            const val TEXT_GAP = 4f
            const val RADIUS = 10f
            const val LIST_GAP = 32f
            const val LEAD_GAP = 24f
        }

        /** `tab.specimen.case`: the styleguide specimen draws this tabCase. */
        const val SPECIMEN_CASE = 1
    }

    // ── frames (#EXP-1493) ─────────────────────────────────────────────────

    enum class FrameKind(val wire: String) { Plan("plan"), Gallery("gallery"), Surface("surface"), Title("title") }

    /** THE viewer's ONE step per story (#EXP-1493): a plan's cover, a
     *  gallery (the pictures swipe inside the frame), a surface, or a bare title. */
    data class Frame(
        /** The story's index in the ring and its id. */
        val story: Int,
        val storyId: String,
        val kind: FrameKind,
        /** The story's resolved pictures (a plan's cover = the first). */
        val pictures: Int,
        val title: String?,
        val description: String?,
        val footer: Footer,
        /** Per picture on a gallery (the frame lasts pictures × it), else the frame's. */
        val autoAdvanceMs: Long?,
    )

    /** A story's frame: a plan = its cover; else pictures win (a gallery: the
     *  surface stays in the chat), else the surface, else the bare title. */
    private fun frameOf(story: Story, at: Int) = Frame(
        story = at,
        storyId = story.id,
        kind = when {
            story.kind == Kind.Plan -> FrameKind.Plan
            story.pictures.isNotEmpty() -> FrameKind.Gallery
            story.surface -> FrameKind.Surface
            else -> FrameKind.Title
        },
        pictures = story.pictures.size,
        title = story.title,
        description = story.description,
        footer = story.footer,
        autoAdvanceMs = story.autoAdvanceMs,
    )

    /** THE viewer's steps over a ring (stories.json `frameCases`): ONE frame
     *  per story, in ring order. */
    fun storyFrames(stories: List<Story>): List<Frame> = stories.mapIndexed { at, story -> frameOf(story, at) }

    /** The frame index of a story; -1 = none. A thumbnail opens its story's
     *  frame with the gallery at that picture (the host's state). */
    fun storyFrameIndex(frames: List<Frame>, storyId: String): Int = frames.indexOfFirst { it.storyId == storyId }

    /** THE STORY ROW of the chats header (#EXP-1461, `rowCases`): ONLY
     *  rings. [StoryRow.bubbles] = the agent bubbles (listAgents order) whose
     *  agent has a ring, each wearing it ([StoryRow.byAgent], first wins);
     *  [StoryRow.rest] = the rings no bubble wears (the root rings, an agent
     *  ring with no bubble), in ring order. A bubble without a ring is not
     *  drawn; both empty = no row at all. */
    data class StoryRow(
        val bubbles: List<String>,
        val byAgent: Map<String, StoryRing>,
        val rest: List<StoryRing>,
    ) {
        val isEmpty: Boolean get() = bubbles.isEmpty() && rest.isEmpty()
    }

    fun storyRow(bubbleAgentIds: List<String>, rings: List<StoryRing>): StoryRow {
        val ids = bubbleAgentIds.toSet()
        val byAgent = LinkedHashMap<String, StoryRing>()
        val rest = mutableListOf<StoryRing>()
        for (ring in rings) {
            val agentId = ring.agentId
            if (agentId != null && agentId in ids && agentId !in byAgent) byAgent[agentId] = ring else rest += ring
        }
        val bubbles = bubbleAgentIds.distinct().filter { it in byAgent }
        return StoryRow(bubbles, byAgent, rest)
    }

    data class ViewerSize(val width: Int, val height: Int)

    /** THE wide viewer's modal size (#EXP-1463, `sizeCases`): a picture with
     *  a known size = fitted (contain) into the viewport less the inset, at
     *  least [Viewer.MIN_WIDTH] wide; anything else = the fixed frame, capped
     *  by the same box. Phones never call it (full screen). */
    fun storyViewerSize(pictureWidth: Double?, pictureHeight: Double?, viewportWidth: Double, viewportHeight: Double): ViewerSize {
        val boxWidth = maxOf(0.0, viewportWidth - 2 * Viewer.INSET_X)
        val boxHeight = maxOf(0.0, viewportHeight - 2 * Viewer.INSET_Y)
        if (pictureWidth == null || pictureHeight == null || pictureWidth == 0.0 || pictureHeight == 0.0) {
            return ViewerSize(
                minOf(Viewer.WIDTH.toDouble(), boxWidth).let(::jsRound),
                minOf(Viewer.HEIGHT.toDouble(), boxHeight).let(::jsRound),
            )
        }
        val scale = minOf(boxWidth / pictureWidth, boxHeight / pictureHeight)
        val width = jsRound(pictureWidth * scale)
        return ViewerSize(
            minOf(boxWidth, maxOf(Viewer.MIN_WIDTH.toDouble(), width.toDouble())).let(::jsRound),
            jsRound(pictureHeight * scale),
        )
    }

    /** JS `Math.round` (half up, toward +inf). */
    private fun jsRound(value: Double): Int = kotlin.math.floor(value + 0.5).toInt()
}
