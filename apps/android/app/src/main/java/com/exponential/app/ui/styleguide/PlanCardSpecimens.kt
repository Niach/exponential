package com.exponential.app.ui.styleguide

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Color as AndroidColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import coil3.SingletonImageLoader
import coil3.asImage
import coil3.memory.MemoryCache
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.PlanAskCard
import com.exponential.app.ui.thread.ThreadAskCard
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/**
 * EXP-1280 (shot `sg_plan-card`, the iOS set's twin): fixtures/session-thread.json
 * case "EXP-1280: a rich plan card…" — the PENDING plan n1 (pictures inline, a
 * picture-tile question with a picture-less tile, a multi list question,
 * the inline Exponential UI surface), the ANSWERED multi question n2
 * (closed: its picks highlighted) and the APPROVED plan n3 (its feedback
 * line) — drawn by the REAL [ThreadAskCard] under plan-card.json
 * `copy.specimen`'s captions. NEW-6 (N-1): the pending plan has two
 * questions, so it is STEPPED ("Question 1 of 2" + Next) and is drawn again
 * on its LAST step (the decision block), like the desktop specimen. The pictures are THE stand-ins ×4 (web
 * `plan-card-specimen.tsx` TONES), drawn here and seeded into Coil's memory
 * cache under the exact `/api/attachments/att-N` key a signed-out load uses.
 */
@Composable
internal fun PlanCardSpecimens() {
    val context = LocalContext.current
    // Seeded ONCE, before the tiles below load (RememberReturnType: a
    // remember never returns Unit, so it yields the done flag).
    remember(context) {
        seedSpecimenPictures(context)
        true
    }
    val rows = remember {
        listOf(
            message("n1", PENDING_PLAN, answeredAt = null, consumedAt = null, createdAt = "2026-10-13T10:01:00.000Z"),
            message(
                "n2",
                ANSWERED_QUESTION,
                answeredAt = "2026-10-13T10:03:00.000Z",
                consumedAt = "2026-10-13T10:03:05.000Z",
                createdAt = "2026-10-13T10:02:00.000Z",
            ),
            message(
                "n3",
                APPROVED_PLAN,
                answeredAt = "2026-10-13T10:04:00.000Z",
                consumedAt = "2026-10-13T10:04:00.000Z",
                createdAt = "2026-10-13T10:04:00.000Z",
            ),
        ).mapNotNull(SessionTree::askRow)
    }
    val captions = listOf(
        PlanCard.Copy.SPECIMEN_PENDING,
        PlanCard.Copy.SPECIMEN_ANSWERED,
        PlanCard.Copy.SPECIMEN_APPROVED,
    )
    rows.forEachIndexed { index, row ->
        Text(
            captions.getOrElse(index) { "" },
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        ThreadAskCard(row, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> })
        val plan = row as? SessionTree.ThreadRow.Plan
        if (plan != null && PlanCard.stepperApplies(true, plan.decision, plan.questions.size)) {
            PlanAskCard(
                plan,
                onAnswer = { _, _, _ -> },
                onDecidePlan = { _, _, _ -> },
                initialStep = plan.questions.size - 1,
            )
        }
    }
}

/** THE stand-in palette ×4 (web TONES): a diagonal gradient per picture. */
private val SPECIMEN_TONES = mapOf(
    "att-1" to (0xFF3F3F46.toInt() to 0xFF52525B.toInt()),
    "att-2" to (0xFF1E3A5F.toInt() to 0xFF3B82F6.toInt()),
    "att-3" to (0xFF14532D.toInt() to 0xFF22C55E.toInt()),
)

/** Draws the three 800×500 stand-ins (the gradient, a 35% white bar, a 12%
 *  white panel) into the app ImageLoader's memory cache. */
private fun seedSpecimenPictures(context: Context) {
    val cache = SingletonImageLoader.get(context).memoryCache ?: return
    SPECIMEN_TONES.forEach { (id, tone) ->
        val bitmap = Bitmap.createBitmap(800, 500, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        paint.shader = LinearGradient(0f, 0f, 800f, 500f, tone.first, tone.second, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, 800f, 500f, paint)
        paint.shader = null
        paint.color = AndroidColor.argb(89, 255, 255, 255)
        canvas.drawRoundRect(RectF(48f, 48f, 368f, 76f), 6f, 6f, paint)
        paint.color = AndroidColor.argb(31, 255, 255, 255)
        canvas.drawRoundRect(RectF(48f, 100f, 752f, 452f), 12f, 12f, paint)
        cache[MemoryCache.Key("/api/attachments/$id")] = MemoryCache.Value(bitmap.asImage())
    }
}

private fun message(id: String, card: String, answeredAt: String?, consumedAt: String?, createdAt: String) =
    SessionTree.Message(
        id = id,
        sessionId = "r2",
        authorKind = "agent",
        authorId = null,
        body = "",
        card = Json.parseToJsonElement(card) as JsonObject,
        answeredAt = answeredAt,
        consumedAt = consumedAt,
        createdAt = createdAt,
    )

// The fixture case's three cards, verbatim.
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

private val APPROVED_PLAN = """
{"kind":"plan","plan":"1. Old","decision":"approved","fresh":false,"feedback":"Go.",
 "answers":{"q":["Yes",7]},
 "questions":[{"id":"q","prompt":"Go?","multi":false,"custom":true,"options":[{"label":"Yes"}]}],
 "surface":{"nope":true}}
""".trimIndent()
