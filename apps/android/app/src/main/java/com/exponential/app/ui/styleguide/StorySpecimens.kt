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
 * S-3 STORIES (stories.json, styleguide `story-ring` / `story-viewer`; the
 * web `StoryRingSpecimen` / `StoryViewerSpecimen` islands): the ring row over
 * static rings (the Assistant's, a waiting chat, an unseen result, a seen
 * one) with single rings and THE progress under it, and THE viewer opened on
 * a draft question carrying an Exponential UI surface and two choices.
 * Capture only (StyleguideScreenshotsTest `captureOnlyScreens`).
 */

const val StoryRingSpecimenTestId = "story-ring-specimen"
const val StoryViewerSpecimenTestId = "story-viewer-specimen"

private fun story(
    id: String,
    session: String,
    kind: Stories.Kind,
    age: String,
    title: String?,
    waiting: Boolean = false,
    seen: Boolean = false,
    surface: Boolean = false,
    text: String? = null,
    footer: Stories.Footer = when (kind) {
        Stories.Kind.Plan -> Stories.Footer.Plan
        Stories.Kind.Question -> Stories.Footer.Reply
        Stories.Kind.Result -> Stories.Footer.None
    },
) = Stories.Story(
    id = id,
    sessionId = "s-$id",
    rootId = "r-$id",
    sessionName = session,
    kind = kind,
    waiting = waiting,
    seen = seen,
    createdAt = 0,
    meta = "${Stories.Copy.kind(kind)} · $age",
    title = title,
    pictures = emptyList(),
    surface = surface,
    text = text,
    footer = footer,
    autoAdvanceMs = if (kind == Stories.Kind.Result) Stories.Constants.AUTO_ADVANCE_MS else null,
)

private val RINGS: List<Stories.StoryRing> = listOf(
    Stories.StoryRing(
        key = "agent:assistant", agentId = "assistant", rootId = null, title = "Assistant", assistant = true,
        waiting = 1, unseen = true,
        stories = listOf(
            story(
                "p1", "Fix the sign-in redirect", Stories.Kind.Plan, "4m",
                title = "Move the redirect into the auth callback", waiting = true,
                text = "Move the redirect into the auth callback\n\n1. Read the `next` param in `/auth/callback`.\n2. Drop the client-side redirect.",
            ),
            story("r1", "Fix the sign-in redirect", Stories.Kind.Result, "12m", title = "Sign-in on the iPad"),
        ),
    ),
    Stories.StoryRing(
        key = "root:r2", agentId = null, rootId = "r2", title = "Board header polish", assistant = false,
        waiting = 1, unseen = true,
        stories = listOf(story("q1", "Board header polish", Stories.Kind.Question, "1h", title = "Which accent for the header?", waiting = true)),
    ),
    Stories.StoryRing(
        key = "agent:ag2", agentId = "ag2", rootId = null, title = "Designer", assistant = false,
        waiting = 0, unseen = true,
        stories = listOf(story("r2", "Empty states", Stories.Kind.Result, "2h", title = "New empty states", surface = true)),
    ),
    Stories.StoryRing(
        key = "root:r3", agentId = null, rootId = "r3", title = "Ship the login flow", assistant = false,
        waiting = 0, unseen = false,
        stories = listOf(
            story("r3", "Ship the login flow", Stories.Kind.Result, "3h", title = "Login flow shipped", seen = true),
            story("r4", "Ship the login flow", Stories.Kind.Result, "2h", title = "Release notes", seen = true),
        ),
    ),
)

private fun avatar(ring: Stories.StoryRing): StoryAvatar = when (ring.agentId) {
    "assistant" -> StoryAvatar(SessionTree.ListCopy.AGENT_DEFAULT_ICON, SessionTree.ListCopy.AGENT_DEFAULT_COLOR)
    "ag2" -> StoryAvatar("bot", "#6366f1")
    else -> StoryAvatar(null, null)
}

/** `story-ring`: the ring row, the rings alone at the header's size, THE
 *  progress mid-story. */
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
                    StoryRing(stories = ring.stories, size = 36.dp) { StoryAvatarMark(mark.icon, mark.color) }
                }
            }
            Box(Modifier.padding(horizontal = 16.dp)) { StoryProgress(count = 4, index = 1, fraction = 0.4f) }
        }
    }
}

private val DRAFT_SURFACE: CardSurface = Json.parseToJsonElement(
    """
    {"root": {"id": "root", "component": "Stack", "props": {"gap": "md"}, "children": [
      {"id": "title", "component": "Text", "props": {"text": "Board header"}, "children": []},
      {"id": "accent", "component": "ChoiceTiles", "props": {"name": "accent", "options": [
        {"label": "Indigo", "value": "indigo"}, {"label": "Emerald", "value": "emerald"}],
        "value": {"path": "/accent"}}, "children": []},
      {"id": "stat", "component": "StatTile", "props": {"label": "Header height", "value": "48 px",
        "delta": "−8 px", "tone": "success"}, "children": []}
    ]}, "data": {"accent": "indigo"}}
    """.trimIndent(),
).jsonObject.let { CardSurface(it.getValue("root").jsonObject, it["data"] as? JsonObject) }

private val DRAFT_QUESTION = SessionTree.ThreadRow.Question(
    id = "q-draft",
    sessionId = "s-draft",
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

private val VIEWER_RINGS: List<Stories.StoryRing> = listOf(
    Stories.StoryRing(
        key = "agent:ag2", agentId = "ag2", rootId = null, title = "Designer", assistant = false,
        waiting = 1, unseen = true,
        stories = listOf(
            story(
                "q-draft", "Board header polish", Stories.Kind.Question, "4m",
                title = DRAFT_QUESTION.question, waiting = true, surface = true, footer = Stories.Footer.Choices,
            ),
            story("r5", "Board header polish", Stories.Kind.Result, "1h", title = "Header spacing"),
        ),
    ),
)

/** `story-viewer`: THE viewer on a draft question (its surface through the
 *  surface card renderer, its two choices in the footer), timer frozen. */
@Composable
fun StoryViewerSpecimenScreen() {
    Box(Modifier.fillMaxSize().testTag(StoryViewerSpecimenTestId)) {
        StoryViewer(
            rings = VIEWER_RINGS,
            startKey = VIEWER_RINGS[0].key,
            rows = mapOf(DRAFT_QUESTION.id to DRAFT_QUESTION),
            avatar = ::avatar,
            onAnswer = { _, _, settled -> settled(false) },
            onDecidePlan = { _, _, settled -> settled(false) },
            onClose = {},
            frozen = true,
        )
    }
}
