package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.CardSurface
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryProgress
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.stories.StoryAvatar
import com.exponential.app.ui.stories.StoryRingStrip
import com.exponential.app.ui.stories.StoryTab
import com.exponential.app.ui.theme.GlassTokens
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import com.exponential.app.ui.stories.StoryViewer
import com.exponential.app.ui.theme.AppBackground
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/*
 * S-3 STORIES (stories.json `specimen`, its `$comment` is the spec; styleguide
 * `story-ring` / `story-viewer`, hardcoded identically on web, desktop and
 * iOS; `cases` replays the same input as "the styleguide specimen"): the
 * rings of [Stories.rings] over [SPECIMEN_INPUT] at the bubble size with
 * their titles, the same rings at `smallRing` px without titles, THE
 * progress mid-story; and THE viewer open on ring 0, story 0 (the waiting
 * question over its Exponential UI surface), the progress frozen at 0.
 * Avatars: an agent ring = its icon in its colour on the muted disc; a chat
 * ring = the chat row's bare brand mark (the roots run claude). Capture only
 * (StyleguideScreenshotsTest `captureOnlyScreens`).
 */

const val StoryRingSpecimenTestId = "story-ring-specimen"
const val StoryViewerSpecimenTestId = "story-viewer-specimen"

/** `specimen.smallRing`. */
private const val SMALL_RING = 36

private const val DRAFT_SURFACE_JSON = """
{"root": {"id": "root", "component": "Stack", "props": {"gap": "md"}, "children": [
      {"id": "title", "component": "Text", "props": {"text": "Board header"}, "children": []},
      {"id": "accent", "component": "ChoiceTiles", "props": {"name": "accent", "options": [
        {"label": "Indigo", "value": "indigo"}, {"label": "Emerald", "value": "emerald"}],
        "value": {"path": "/accent"}}, "children": []},
      {"id": "stat", "component": "StatTile", "props": {"label": "Header height", "value": "48 px",
        "delta": "−8 px", "tone": "success"}, "children": []}
    ]}, "data": {"accent": "indigo"}}
    """

private val DRAFT_SURFACE: CardSurface = Json.parseToJsonElement(DRAFT_SURFACE_JSON)
    .jsonObject.let { CardSurface(it.getValue("root").jsonObject, it["data"] as? JsonObject) }

private fun card(json: String): JsonObject = Json.parseToJsonElement(json).jsonObject

/** `specimen.input`, verbatim. */
private const val NOW = 1_800_000_000_000L

private val SESSIONS = listOf(
    Stories.SessionInput(id = "triage", rootId = "triage", name = "Triage", agentId = "ag1", startedByAgentId = null),
    Stories.SessionInput(id = "board", rootId = "board", name = "Board header polish", agentId = null, startedByAgentId = null),
    Stories.SessionInput(id = "designer", rootId = "designer", name = "Designer", agentId = "ag2", startedByAgentId = null),
    Stories.SessionInput(id = "ship", rootId = "ship", name = "Ship the launch checklist", agentId = null, startedByAgentId = null),
)

private val AGENTS = listOf(
    Stories.AgentInput("ag1", "Triage", assistant = false),
    Stories.AgentInput("ag2", "Designer", assistant = false),
)

/** The agents' marks (`agents[].icon` / `color`). */
private val AGENT_MARKS = mapOf(
    "ag1" to StoryAvatar("bug", "#a855f7"),
    "ag2" to StoryAvatar("bot", "#6366f1"),
)

private fun result(title: String, text: String, description: String? = null) =
    card(
        """{"kind": "surface", "story": true, "title": "$title", "surface": {"root": {"id": "r", "component": "Text", "props": {"text": "$text"}}}""" +
            (description?.let { """, "description": "$it"""" } ?: "") + "}",
    )

private val MESSAGES = listOf(
    Stories.MessageInput(
        id = "triage-plan", sessionId = "triage", authorSessionId = "triage", createdAt = 1_799_999_460_000L, answeredAt = null,
        card = card("""{"kind": "plan", "plan": "## Close the duplicates\n\nMerge FB-8 into FB-3 and label the rest.", "decision": null}"""),
    ),
    Stories.MessageInput(
        id = "triage-result", sessionId = "triage", authorSessionId = "triage", createdAt = 1_799_998_200_000L, answeredAt = null,
        card = result("Today's triage", "3 labelled"),
    ),
    Stories.MessageInput(
        id = "board-question", sessionId = "board", authorSessionId = "board", createdAt = 1_799_999_760_000L, answeredAt = null,
        card = card(
            """{"kind": "question", "question": "Which accent for the board header?", "options": ["Indigo", "Emerald"],
            "optionDescriptions": ["The brand accent", "Matches the merged state"], "custom": false,
            "surface": $DRAFT_SURFACE_JSON}""",
        ),
    ),
    Stories.MessageInput(
        id = "designer-result", sessionId = "designer", authorSessionId = "designer", createdAt = 1_799_996_400_000L, answeredAt = null,
        card = result("Spacing review", "No drift", "Paddings match on all four platforms."),
    ),
    Stories.MessageInput(
        id = "ship-result", sessionId = "ship", authorSessionId = "ship", createdAt = 1_799_992_800_000L, answeredAt = null,
        card = result("Checklist done", "12 of 12"),
    ),
)

private val READS = listOf(Stories.ReadInput("ship", readAt = 1_799_996_400_000L, storySeenAt = null))

/** `stories(specimen.input)`: Board header polish, Triage, Designer, Ship the launch checklist. */
private val RINGS: List<Stories.StoryRing> =
    Stories.rings(now = NOW, sessions = SESSIONS, agents = AGENTS, messages = MESSAGES, reads = READS)

/** Agent ring = its mark; chat ring = the bare claude brand mark (the specimen roots run `cli` claude). */
private fun avatar(ring: Stories.StoryRing): StoryAvatar =
    ring.agentId?.let { AGENT_MARKS[it] } ?: StoryAvatar(null, null, cli = "claude")

/** The specimen's rings + avatars for the `chat-list` specimen's bubbles row
 *  (ag1 / ag2 = `bubbleCases[0]`'s Triage / Designer). */
internal val STORY_RING_SPECIMEN: List<Stories.StoryRing> get() = RINGS

internal fun storySpecimenAvatar(ring: Stories.StoryRing): StoryAvatar = avatar(ring)

/** `story-ring`: the rings with their titles, the same rings small, THE
 *  progress (`specimen.progress`). No state words. */
@Composable
fun StoryRingSpecimenScreen() {
    AppBackground {
        Column(
            Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 16.dp).testTag(StoryRingSpecimenTestId),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            StoryRingStrip(rings = RINGS, avatar = ::avatar, onOpen = {})
            Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                RINGS.forEach { ring ->
                    val mark = avatar(ring)
                    StoryRing(stories = ring.stories, size = SMALL_RING.dp) { StoryAvatarMark(mark.icon, mark.color, cli = mark.cli) }
                }
            }
            Box(Modifier.padding(horizontal = 16.dp)) { StoryProgress(count = 4, index = 1, fraction = 0.4f) }
        }
    }
}

/** The waiting question's card as THE thread row (the viewer's body + footer). */
private val DRAFT_QUESTION = SessionTree.ThreadRow.Question(
    id = "board-question",
    sessionId = "board",
    question = "Which accent for the board header?",
    options = listOf(
        SessionTree.ThreadRow.QuestionChoice("Indigo", "The brand accent"),
        SessionTree.ThreadRow.QuestionChoice("Emerald", "Matches the merged state"),
    ),
    custom = false,
    answer = null,
    answered = false,
    surface = DRAFT_SURFACE,
)

/** `story-viewer`: THE viewer on ring 0, story 0 (`specimen.viewer`), its
 *  surface through the surface card renderer, its two choices in the footer,
 *  the progress frozen at 0. */
@Composable
fun StoryViewerSpecimenScreen() {
    Box(Modifier.fillMaxSize().testTag(StoryViewerSpecimenTestId)) {
        StoryViewer(
            rings = RINGS,
            startKey = RINGS[0].key,
            rows = mapOf(DRAFT_QUESTION.id to DRAFT_QUESTION),
            avatar = ::avatar,
            onAnswer = { _, _, settled -> settled(false) },
            onClose = {},
            frozen = true,
        )
    }
}

// ── story-tab (stories.json `tab.specimen` = tabCases[1]) ───────────────────

const val StoryTabSpecimenTestId = "story-tab-specimen"

private val TAB_SESSIONS = listOf(
    Stories.SessionInput(id = "r1", rootId = "r1", name = "Batch-edit labels", agentId = null, startedByAgentId = null),
    Stories.SessionInput(id = "c1", rootId = "r1", name = "Web draft", agentId = null, startedByAgentId = null),
    Stories.SessionInput(id = "r2", rootId = "r2", name = "Other chat", agentId = null, startedByAgentId = null),
)

private const val CARD_SURFACE = """{"root": {"id": "c", "component": "Card"}}"""

private fun tabMessage(id: String, session: String, created: Long, answered: Long?, json: String, author: String = session) =
    Stories.MessageInput(id = id, sessionId = session, authorSessionId = author, createdAt = created, answeredAt = answered, card = card(json))

/** `tabCases[1].input.messages`, verbatim. */
private val TAB_MESSAGES = listOf(
    tabMessage("q-wait", "r1", 1_799_999_400_000L, null, """{"kind": "question", "question": "Ship the picker as is?", "options": ["Ship", "Polish"], "answer": null}"""),
    tabMessage(
        "res-web", "c1", 1_799_992_800_000L, null,
        """{"kind": "surface", "surface": $CARD_SURFACE, "title": "Label picker", "pictures": [
            {"label": "web picker", "attachmentId": "att-w", "width": 1600, "height": 1000, "caption": "Mixed state"},
            {"label": "ios picker", "attachmentId": "att-i", "width": 1179, "height": 2556, "caption": null},
            {"label": "android picker", "attachmentId": null, "width": null, "height": null, "caption": null}]}""",
    ),
    tabMessage("plan-ok", "r1", 1_799_892_000_000L, 1_799_895_600_000L, """{"kind": "plan", "plan": "## Batch-edit labels\n\nOne request per change.", "decision": "approved"}"""),
    tabMessage("plan-old", "r1", 1_799_888_400_000L, 1_799_892_000_000L, """{"kind": "plan", "plan": "Old plan", "decision": "superseded"}"""),
    tabMessage("plan-no", "c1", 1_799_982_000_000L, 1_799_985_600_000L, """{"kind": "plan", "plan": "Drop the undo step", "decision": "rejected"}"""),
    tabMessage("q-done", "c1", 1_799_989_200_000L, 1_799_989_800_000L, """{"kind": "question", "question": "Which board?", "options": [], "answer": "Mobile App"}"""),
    tabMessage("old-result", "r1", 1_799_222_400_000L, null, """{"kind": "surface", "story": true, "surface": $CARD_SURFACE, "title": "First draft"}"""),
    tabMessage("status", "r1", 1_799_999_940_000L, null, """{"kind": "surface", "surface": $CARD_SURFACE, "title": "Working"}"""),
    tabMessage("relay", "c1", 1_799_998_800_000L, null, """{"kind": "question", "question": "Engine ask", "options": ["Yes"], "questionId": "q1"}"""),
    tabMessage("mirror", "r1", 1_799_999_100_000L, null, """{"kind": "ask", "sessionId": "c1", "messageId": "plan-no"}""", author = "c1"),
    tabMessage("elsewhere", "r2", 1_799_999_940_000L, null, """{"kind": "question", "question": "Not here", "options": ["A"]}"""),
)

/** `storyTab(tabCases[1].input)`: q-wait, res-web, q-done, plan-no, plan-ok, old-result. */
private val TAB_RING: Stories.StoryRing? = Stories.storyTab(
    now = NOW,
    sessions = TAB_SESSIONS,
    agents = emptyList(),
    messages = TAB_MESSAGES,
    reads = listOf(Stories.ReadInput("r1", readAt = 1_799_996_400_000L, storySeenAt = null)),
    rootId = "r1",
)

/** `story-tab`: THE Stories tab body over tabCases[1]'s ring (the phone
 *  size, no PR block) in the web specimen's card; the pictures are
 *  placeholders (the thumbnail's muted fill alone). */
@Composable
fun StoryTabSpecimenScreen() {
    val shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp)
    AppBackground {
        Box(Modifier.fillMaxSize().statusBarsPadding().padding(16.dp)) {
            Box(
                Modifier
                    .testTag(StoryTabSpecimenTestId)
                    .specimenCard(shape)
                    .padding(20.dp),
            ) {
                StoryTab(ring = TAB_RING, onOpen = { _, _ -> })
            }
        }
    }
}

private fun Modifier.specimenCard(shape: androidx.compose.ui.graphics.Shape): Modifier =
    this
        .background(com.exponential.app.ui.theme.DesignTokens.Palette.Background, shape)
        .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
