package com.exponential.app.ui.styleguide

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.ThreadAskCard
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/**
 * EXP-1280 (shot `sg_plan-card`, the iOS set's twin): fixtures/session-thread.json
 * case "EXP-1280: a rich plan card…" — the PENDING plan (pictures inline, a
 * picture-tile question with a picture-less tile, a multi list question,
 * the inline Exponential UI surface) and the ANSWERED multi question
 * (closed: its picks highlighted) — drawn by the REAL [ThreadAskCard].
 */
@Composable
internal fun PlanCardSpecimens() {
    val rows = remember {
        listOf(
            message("n1", PENDING_PLAN, answeredAt = null),
            message("n2", ANSWERED_QUESTION, answeredAt = "2026-10-13T10:03:00.000Z"),
        ).mapNotNull(SessionTree::askRow)
    }
    rows.forEachIndexed { index, row ->
        Text(
            if (index == 0) "Pending plan" else "Answered question",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        ThreadAskCard(row, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> })
    }
}

private fun message(id: String, card: String, answeredAt: String?) = SessionTree.Message(
    id = id,
    sessionId = "r2",
    authorKind = "agent",
    authorId = null,
    body = "",
    card = Json.parseToJsonElement(card) as JsonObject,
    answeredAt = answeredAt,
    consumedAt = answeredAt,
    createdAt = "2026-10-13T10:01:00.000Z",
)

// The fixture case's two cards, verbatim.
private val PENDING_PLAN = """
{"kind":"plan","plan":"# Plan\n\n1. Lift the partition\n\n![before](/api/attachments/att-1)\n\n![after](upload:after)",
 "pictures":[{"label":"before","attachmentId":"att-1","width":1440,"height":900,"caption":"Before"},
  {"label":"after","attachmentId":null,"width":null,"height":null,"caption":null},
  {"label":"two","attachmentId":"att-2","width":800,"height":500},
  {"label":"one","attachmentId":null,"width":null,"height":null}],
 "questions":[{"id":"layout","prompt":"Which layout?","multi":false,"custom":false,"options":[
   {"label":"Two columns","description":"Pictures beside the text","picture":"two"},
   {"label":"One column","picture":"one"},{"label":"Neither"}]},
  {"id":"platforms","prompt":"Platforms","multi":true,"options":[{"label":"iOS"},{"label":"Android","description":""}]},
  {"id":"broken","options":[]},"junk"],
 "surface":{"root":{"id":"s","component":"Stack","props":{"direction":"vertical","gap":"sm"},"children":[
   {"id":"t","component":"Text","props":{"text":"Pick the accent"}},
   {"id":"c","component":"ChoiceTiles","props":{"name":"accent","options":[{"label":"Blue","value":"blue"},{"label":"Green","value":"green"}],"value":{"path":"/accent"}}}]},
  "data":{"accent":"blue"}},
 "decision":null,"fresh":null,"feedback":null,"answers":null,"surfaceData":null}
""".trimIndent()

private val ANSWERED_QUESTION = """
{"kind":"question","question":"Which platforms first?","options":["Web","iOS","Android"],
 "optionDescriptions":["The styleguide first",null,""],"optionPictures":["web",null,"ghost"],
 "pictures":[{"label":"web","attachmentId":"att-3","width":1200,"height":750}],
 "multi":true,"custom":false,"surface":null,"answer":"Web, Android","answers":["Web","Android"],"surfaceData":{"size":"lg"}}
""".trimIndent()
