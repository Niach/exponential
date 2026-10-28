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

private fun result(title: String, text: String) =
    card("""{"kind": "surface", "story": true, "title": "$title", "surface": {"root": {"id": "r", "component": "Text", "props": {"text": "$text"}}}}""")

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
        card = result("Spacing review", "No drift"),
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
            onDecidePlan = { _, _, settled -> settled(false) },
            onClose = {},
            frozen = true,
        )
    }
}
