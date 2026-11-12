package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EXP-1493 → #EXP-1662 THE PLAN TAB ×4 (`packages/domain-contract/fixtures/plan-page.json`,
 * TS reference `src/plan-page.ts`, iOS ExpCore `PlanPage.swift`, desktop
 * `domain::plan_page`): the expanded form of THE plan card, a tab of its
 * chat. A plan's markdown split into blocks (the title, prose, h2 sections,
 * picture groups), its outline, its counts line, the plan the tab shows, its
 * row label and where a session's waiting plan opens.
 * `PlanPageTest` replays every `*Cases` array.
 */
object PlanPage {

    object Copy {
        const val TITLE = "Plan"
        const val OPEN_PLAN = "Open plan"
        const val OPEN_FULL = "Open full plan"
        const val OPEN_CHAT = "Open chat"
        const val OUTLINE = "Outline"
        const val QUESTIONS = "Questions"
        /** singular, plural. */
        val SECTIONS = listOf("section", "sections")
        val PICTURES = listOf("picture", "pictures")
        val QUESTIONS_COUNT = listOf("question", "questions")
        const val NEEDS_YOU = "Needs you"
        const val ROW_PREFIX = "Plan: "
    }

    /** dp; Android = the `phone` form factor. */
    object Layout {
        const val OUTLINE_WIDTH = 240f
        const val COLUMN_MAX_WIDTH = 720f
        const val COLUMN_PADDING_WIDE = 28f
        const val COLUMN_PADDING_PHONE = 18f
        const val BLOCK_GAP = 14f
        const val OUTLINE_ROW_HEIGHT = 32f
        const val OUTLINE_ACTIVE_BAR = 2f
        const val TITLE_SIZE = 28f
        const val ROW_HEIGHT = 44f
        const val TAB_MAX_WIDTH = 260f
        /** #FEED-191: the questions rail, WIDE only (phones keep the panel). */
        const val RAIL_WIDTH = 360f
        /** #EXP-1786: the document's readable floor ([planPaneLayout]). */
        const val DOCUMENT_MIN_WIDTH = 360f
    }

    /** #EXP-1662: THE questions, floating over the document's foot. */
    object Panel {
        const val MAX_WIDTH = 720f
        const val GAP = 16f
        const val PADDING = 16f
        const val RADIUS = 16f
        const val MAX_HEIGHT_FRACTION = 0.4f
    }

    object Mosaic {
        const val GAP = 10f
        const val LARGE_FRACTION = 0.6f
        const val ASPECT = 1.6f
        const val RADIUS = 10f
    }

    object TestIds {
        const val PAGE = "plan-page"
        const val OUTLINE = "plan-outline"
        const val OUTLINE_ROW = "plan-outline-row"
        const val DOCUMENT = "plan-document"
        const val RAIL = "plan-rail"
        const val MOSAIC = "plan-mosaic"
        const val CAROUSEL = "plan-carousel"
        const val OPEN_PLAN = "open-plan"
        const val OUTLINE_TOGGLE = "plan-outline-toggle"
        const val TAB = "chat-tab-plan"
    }

    const val ROUTE = "/t/\$teamSlug/sessions/\$sessionId/plan/\$messageId"

    /** THE styleguide specimen ×4 (`specimen`): the chat's name and the
     *  active outline row; its card lives in the specimen. */
    object Specimen {
        const val CHAT_NAME = "Editor-first app creation, eApps MCP"
        const val ACTIVE_SECTION = "part-a-editor-first-creation"
    }

    // ── model ──────────────────────────────────────────────────────────────

    data class Picture(val alt: String, val src: String)

    sealed interface Block {
        /** #EXP-1662: a leading `# ` line, the document's big title. */
        data class Title(val text: String) : Block
        data class Markdown(val text: String) : Block
        data class Section(val id: String, val title: String) : Block
        data class Pictures(val pictures: List<Picture>) : Block
    }

    data class Section(val id: String, val title: String, val pictures: Int)

    data class Counts(val sections: Int, val pictures: Int, val questions: Int)

    private val PICTURE_LINE = Regex("""^\s*!\[([^\]]*)\]\(([^)\s]+)\)\s*$""")
    private val PICTURE_ANY = Regex("""!\[([^\]]*)\]\(([^)\s]+)\)""")
    private val H1 = Regex("""^#\s+(.+?)\s*#*\s*$""")
    private val H2 = Regex("""^##\s+(.+?)\s*#*\s*$""")
    private val FENCE = Regex("""^\s*(```|~~~)""")
    private val LINK = Regex("""\[([^\]]*)\]\([^)]*\)""")
    private const val PENDING = "upload:"

    private fun isFence(line: String) = FENCE.containsMatchIn(line)

    /** A heading's anchor: lowercase, runs of anything but a-z0-9 = `-`. */
    fun sectionSlug(title: String): String {
        val slug = title.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-')
        return slug.ifEmpty { "section" }
    }

    private fun plainTitle(raw: String): String =
        raw.replace(LINK) { it.groupValues[1] }
            .replace("**", "")
            .replace("__", "")
            .replace("`", "")
            .trim()

    /** THE plan page's blocks (`blockCases`): a leading `# ` line = the `title`
     *  (#EXP-1662; every other `# ` stays in the prose), every `## ` line
     *  outside a code fence = a `section` (anchor deduped `-2`, `-3`…), a run of two or more
     *  picture-only lines (blank lines between allowed) = ONE `pictures`
     *  block; a lone picture stays in the prose; a pending `upload:` picture
     *  is dropped. Prose blocks are trimmed of blank edge lines; empty = dropped. */
    fun blocks(markdown: String): List<Block> {
        val blocks = mutableListOf<Block>()
        val lines = markdown.split("\n").toMutableList()
        val first = lines.indexOfFirst { it.isNotBlank() }
        val title = if (first >= 0) H1.matchEntire(lines[first]) else null
        if (title != null) {
            blocks += Block.Title(plainTitle(title.groupValues[1]))
            lines.removeAt(first)
        }
        val seen = HashMap<String, Int>()
        var prose = mutableListOf<String>()
        var run = mutableListOf<Picture>()
        var runLines = mutableListOf<String>()
        var fenced = false

        fun flushProse() {
            while (prose.isNotEmpty() && prose.first().isBlank()) prose.removeAt(0)
            while (prose.isNotEmpty() && prose.last().isBlank()) prose.removeAt(prose.lastIndex)
            if (prose.isNotEmpty()) blocks += Block.Markdown(prose.joinToString("\n"))
            prose = mutableListOf()
        }
        fun flushRun() {
            if (run.size >= 2) {
                flushProse()
                blocks += Block.Pictures(run)
            } else {
                prose.addAll(runLines)
            }
            run = mutableListOf()
            runLines = mutableListOf()
        }

        for (line in lines) {
            if (isFence(line)) {
                flushRun()
                fenced = !fenced
                prose += line
                continue
            }
            if (fenced) {
                prose += line
                continue
            }
            val picture = PICTURE_LINE.matchEntire(line)
            if (picture != null) {
                val src = picture.groupValues[2]
                if (src.startsWith(PENDING)) continue
                run += Picture(picture.groupValues[1], src)
                runLines += line
                continue
            }
            if (line.isBlank() && run.isNotEmpty()) {
                runLines += line
                continue
            }
            flushRun()
            val heading = H2.matchEntire(line)
            if (heading != null) {
                flushProse()
                val title = plainTitle(heading.groupValues[1])
                val base = sectionSlug(title)
                val n = (seen[base] ?: 0) + 1
                seen[base] = n
                blocks += Block.Section(if (n == 1) base else "$base-$n", title)
                continue
            }
            prose += line.replace(PICTURE_ANY) { if (it.groupValues[2].startsWith(PENDING)) "" else it.value }
        }
        flushRun()
        flushProse()
        return blocks
    }

    /** The pictures inside a prose block, outside its code fences. */
    private fun inlinePictures(text: String): List<Picture> {
        val out = mutableListOf<Picture>()
        var fenced = false
        for (line in text.split("\n")) {
            if (isFence(line)) {
                fenced = !fenced
            } else if (!fenced) {
                PICTURE_ANY.findAll(line)
                    .filter { !it.groupValues[2].startsWith(PENDING) }
                    .forEach { out += Picture(it.groupValues[1], it.groupValues[2]) }
            }
        }
        return out
    }

    /** The pictures a block shows (a fenced code block shows none). */
    private fun picturesIn(block: Block): List<Picture> = when (block) {
        is Block.Pictures -> block.pictures
        is Block.Title, is Block.Section -> emptyList()
        is Block.Markdown -> inlinePictures(block.text)
    }

    /** THE outline (`outlineCases`): one row per `section` block, in order,
     *  with the pictures between it and the next section. */
    fun outline(markdown: String): List<Section> {
        val out = mutableListOf<Section>()
        for (block in blocks(markdown)) {
            if (block is Block.Section) {
                out += Section(block.id, block.title, 0)
            } else if (out.isNotEmpty()) {
                val last = out.last()
                out[out.lastIndex] = last.copy(pictures = last.pictures + picturesIn(block).size)
            }
        }
        return out
    }

    /** Every picture of a plan in document order (THE lightbox's list). */
    fun pictures(markdown: String): List<Picture> = blocks(markdown).flatMap(::picturesIn)

    /** A plan card's counts (`countCases`): its h2 sections, its RESOLVED
     *  pictures (attachmentId set), its questions. */
    fun counts(card: JsonObject?): Counts {
        val plan = (card?.get("plan") as? JsonPrimitive)?.takeIf { it.isString }?.content ?: ""
        val pictures = (card?.get("pictures") as? JsonArray).orEmpty().count { raw ->
            ((raw as? JsonObject)?.get("attachmentId") as? JsonPrimitive)?.isString == true
        }
        return Counts(
            sections = outline(plan).size,
            pictures = pictures,
            questions = (card?.get("questions") as? JsonArray)?.size ?: 0,
        )
    }

    /** "5 sections · 10 pictures · 7 questions": zero parts left out, the
     *  singular for one; nothing = "". */
    fun countsLine(counts: Counts): String =
        listOf(counts.sections to Copy.SECTIONS, counts.pictures to Copy.PICTURES, counts.questions to Copy.QUESTIONS_COUNT)
            .filter { it.first > 0 }
            .joinToString(" · ") { (n, words) -> "$n ${words[if (n == 1) 0 else 1]}" }

    enum class TileSize(val wire: String) { Large("large"), Half("half"), Small("small") }

    /** [more] = "+N" on the last small tile when the group has more than it shows. */
    data class MosaicTile(val index: Int, val size: TileSize, val more: Int)

    /** THE wide picture group (eUI `Mosaic`, `mosaicCases`). Phones draw a
     *  carousel instead; kept for the contract. */
    fun mosaicTiles(count: Int): List<MosaicTile> = when {
        count <= 0 -> emptyList()
        count == 1 -> listOf(MosaicTile(0, TileSize.Large, 0))
        count == 2 -> listOf(MosaicTile(0, TileSize.Half, 0), MosaicTile(1, TileSize.Half, 0))
        else -> listOf(
            MosaicTile(0, TileSize.Large, 0),
            MosaicTile(1, TileSize.Small, 0),
            MosaicTile(2, TileSize.Small, count - 3),
        )
    }

    data class Message(
        val id: String,
        val sessionId: String,
        val authorSessionId: String?,
        /** Epoch ms. */
        val createdAt: Long,
        val card: JsonObject?,
    )

    /** Where a session's waiting plan opens (`targetCases`): the NEWEST plan
     *  card the session posted itself (no engine `questionId`, no decision);
     *  null = none (the host opens the session). */
    fun planPageFor(sessionId: String, messages: List<Message>): String? {
        var best: Message? = null
        for (message in messages) {
            if (message.sessionId != sessionId || message.authorSessionId != sessionId) continue
            val card = message.card ?: continue
            if ((card["kind"] as? JsonPrimitive)?.content != "plan") continue
            if (card["decision"] != null && card["decision"] !is JsonNull) continue
            if ((card["questionId"] as? JsonPrimitive)?.isString == true) continue
            val kept = best
            if (kept == null || message.createdAt > kept.createdAt ||
                (message.createdAt == kept.createdAt && message.id > kept.id)
            ) {
                best = message
            }
        }
        return best?.id
    }

    /** A chat's plan card, as the tab reads it. */
    data class TabPlan(val id: String, val decision: String?)

    /** #EXP-1662 / #FEED-191 (`tabCases`): the plan the chat's tab shows
     *  ([plans] oldest first): the newest waiting one, else [kept] while it is
     *  an approved or rejected plan of the chat (the plan the tab showed
     *  during THIS visit of the chat; the screen clears it when the person
     *  leaves), else none: a decided plan leaves the strip. */
    fun planTabTarget(plans: List<TabPlan>, kept: String? = null): String? {
        plans.lastOrNull { it.decision == null }?.let { return it.id }
        val held = kept?.let { id -> plans.firstOrNull { it.id == id } } ?: return null
        return if (held.decision == "approved" || held.decision == "rejected") held.id else null
    }

    /** #EXP-1786: which wide columns the plan body's OWN width holds
     *  (`paneCases`). [outline] false = no outline column; [rail] false = the
     *  questions float ([Panel]). */
    data class PaneLayout(val outline: Boolean, val rail: Boolean)

    /** The three columns while the document keeps [Layout.DOCUMENT_MIN_WIDTH]
     *  beside both, else the document + the rail, else the document alone. */
    fun planPaneLayout(width: Float): PaneLayout = PaneLayout(
        outline = width >= Layout.OUTLINE_WIDTH + Layout.RAIL_WIDTH + Layout.DOCUMENT_MIN_WIDTH,
        rail = width >= Layout.RAIL_WIDTH + Layout.DOCUMENT_MIN_WIDTH,
    )

    /** #FEED-191: the plan tab's lead glyph. [icon] = an icon concept id,
     *  [tone] = `text` (the chip's own text colour) or `approved` (emerald,
     *  the plan row's approved check). */
    data class TabGlyph(val icon: String, val tone: String)

    /** #FEED-191 (`tabGlyphCases`): an approved plan = THE check in emerald
     *  (as its row), anything else = the plan glyph. */
    fun planTabGlyph(decision: String?): TabGlyph =
        if (decision == "approved") TabGlyph("ui-check", "approved") else TabGlyph("coding-plan", "text")

    private val PLAN_PREFIX = Regex("""^plan:\s*""", RegexOption.IGNORE_CASE)

    /** #EXP-1662 (`rowLabelCases`): `Plan: title`, the Conversation row's label. */
    fun planRowLabel(plan: String): String {
        val title = PLAN_PREFIX.replaceFirst(AskDock.askDockTitle(plan), "")
        return if (title.isEmpty()) Copy.TITLE else "${Copy.ROW_PREFIX}$title"
    }

    /** The web path, which redirects to the chat's tab. */
    fun path(teamSlug: String, sessionId: String, messageId: String): String =
        ROUTE.replace("\$teamSlug", teamSlug)
            .replace("\$sessionId", sessionId)
            .replace("\$messageId", messageId)
}
