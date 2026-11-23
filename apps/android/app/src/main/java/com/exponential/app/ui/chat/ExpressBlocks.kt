package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import com.exponential.app.domain.ChatExpress
import com.exponential.app.domain.ChatExpressFfi
import com.exponential.app.ui.components.LocalToaster
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// #EXP-2067 / #EXP-2073 (chat-express.json DRAW ×4): an agent row whose text
// holds Express blocks becomes its segments in order: each prose segment THE
// MessageBubble, each block THE surface card frame + painter (one frame per
// surface of the block), ONE sender group.

/**
 * Where a transcript's block presses go: EVERY action as
 * [ChatExpress.surfaceAction] (a [ChatExpress.ActionResult.TooLarge] = THE
 * failed-action toast). Null = inert (a specimen, a read-only view).
 */
val LocalExpressActions = compositionLocalOf<((ChatExpress.ActionResult) -> Unit)?> { null }

/** The session whose feed draws the rows (a feed row's key `n:<session>:…`). */
val LocalExpressSession = compositionLocalOf<String?> { null }

/** #EXP-2067: a feed narration row's key: a synced tail row (`synced:<id>`)
 *  = `m:<id>`, else `n:<session>:<messageId>` / `e:<session>:<seq>`. */
fun feedExpressRowKey(sessionId: String, messageId: String?, firstSeq: Long): String =
    messageId?.takeIf { it.startsWith("synced:") }?.let { ChatExpress.syncedRowKey(it.removePrefix("synced:")) }
        ?: ChatExpress.feedRowKey(sessionId, messageId, firstSeq)

/** The press sink a host provides: a post goes to [post], a too-large one is THE failed-action toast. */
@Composable
fun rememberExpressActionSink(post: (ChatExpress.ActionResult.Post) -> Unit): (ChatExpress.ActionResult) -> Unit {
    val toaster = LocalToaster.current
    val latest = rememberUpdatedState(post)
    return remember(toaster) {
        { result ->
            when (result) {
                is ChatExpress.ActionResult.Post -> latest.value(result)
                ChatExpress.ActionResult.TooLarge -> toaster.error(ChatExpress.Copy.ACTION_TOO_LARGE)
            }
        }
    }
}

/**
 * A row's segments ([ChatExpress.ExpressRow], kept per [rowKey] so a stream
 * pushes only each block's new suffix); null = the gate fails: the row draws
 * exactly as today (nothing compiled, no SDK loaded).
 */
@Composable
fun rememberExpressSegments(rowKey: String, text: String, final: Boolean): List<ChatExpress.RowSegment>? {
    val row = remember(rowKey) { ChatExpress.ExpressRow(rowKey, ChatExpressFfi) }
    return remember(row, text, final) {
        // A native library that failed to load = today's text.
        runCatching { row.update(text, final) }.getOrNull()
    }
}

/** The synced message id behind a row key (`m:<id>`), else null (a live feed row). */
fun expressMessageId(rowKey: String): String? = rowKey.takeIf { it.startsWith("m:") }?.removePrefix("m:")

/**
 * The segments of one row, [ChatExpress.BLOCK_GAP] apart. [prose] draws a
 * prose segment as THE bubble: [first] = the row's first segment (name,
 * avatar, tail), [last] = its LAST prose segment (the `HH:mm`).
 */
@Composable
fun ExpressSegments(
    rowKey: String,
    segments: List<ChatExpress.RowSegment>,
    modifier: Modifier = Modifier,
    /** The row ARRIVED while the transcript was open (its blocks enter). */
    arrived: Boolean = false,
    prose: @Composable (text: String, first: Boolean, last: Boolean) -> Unit,
) {
    val lastProse = segments.indexOfLast { it is ChatExpress.RowSegment.Prose }
    // The surfaces this row showed when it was first drawn: they never enter;
    // one created after that (live) does.
    val seen = remember(rowKey) {
        segments.filterIsInstance<ChatExpress.RowSegment.Block>().flatMapTo(HashSet()) { b -> b.surfaces.map { "${b.key}/$it" } }
    }
    Column(
        modifier = modifier.fillMaxWidth().testTag("express-row"),
        verticalArrangement = Arrangement.spacedBy(ChatExpress.BLOCK_GAP.dp),
    ) {
        segments.forEachIndexed { index, segment ->
            when (segment) {
                is ChatExpress.RowSegment.Prose -> prose(segment.text, index == 0, index == lastProse)
                is ChatExpress.RowSegment.Block -> ExpressBlock(
                    block = segment,
                    messageId = expressMessageId(rowKey),
                    enters = { id -> arrived || "${segment.key}/$id" !in seen },
                )
            }
        }
    }
}

/**
 * One block: a frame per surface (creation order, [ChatExpress.BLOCK_GAP]
 * apart); `open` with no surface yet = THE skeleton in the frame; `failed` /
 * `tooMany` / `tooLarge` = its surfaces (if any) then THE inline marker.
 */
@Composable
fun ExpressBlock(block: ChatExpress.RowSegment.Block, messageId: String?, enters: (surfaceId: String) -> Boolean) {
    Column(
        modifier = Modifier.fillMaxWidth().testTag("express-block"),
        verticalArrangement = Arrangement.spacedBy(ChatExpress.BLOCK_GAP.dp),
    ) {
        for (surfaceId in block.surfaces) {
            ExpressSurface(block, surfaceId, messageId, enters(surfaceId))
        }
        if (block.surfaces.isEmpty() && block.status == ChatExpress.BlockStatus.Open) {
            SurfaceCardFrame(key = "${block.key}/skeleton", model = null, animate = false, placeholder = true)
        }
        if (block.marked) FeedMarkerRow(ChatExpress.Copy.BLOCK_FAILED, testTag = "express-block-failed")
    }
}

@Composable
private fun ExpressSurface(block: ChatExpress.RowSegment.Block, surfaceId: String, messageId: String?, animate: Boolean) {
    val actions = LocalExpressActions.current
    val origin = ChatExpress.ActionOrigin(surfaceId, block.key, messageId)
    val host = rememberCardHostFactory(
        if (actions == null) {
            null
        } else {
            { name, context, userMessage -> ChatExpress.surfaceAction(name, context, userMessage, origin)?.let(actions) }
        },
    )
    val model = rememberBlockSurfaceModel(block, surfaceId, host)
    SurfaceCardFrame(key = "${block.key}/$surfaceId", model = model, animate = animate, stagger = false)
}

/**
 * The renderer state of [surfaceId] in [block]'s stream: a [SurfaceModel]
 * fed the block's messages for that surface in order; a stream that grows
 * applies only its new messages, a restart ([ChatExpress.RowSegment.Block.generation])
 * starts over.
 */
@Composable
private fun rememberBlockSurfaceModel(block: ChatExpress.RowSegment.Block, surfaceId: String, host: () -> HostPlugin): SurfaceModel? {
    val applied = remember(block.key, surfaceId, block.generation) { intArrayOf(0) }
    val model = remember(block.key, surfaceId, block.generation) {
        runCatching {
            SurfaceModel(id = surfaceId, options = SurfaceOptions(), host = host()) { error("set by ExponentialSurface") }
                .also { m -> applied[0] = m.applyBlock(block.messages, surfaceId, 0) }
        }.getOrNull()
    }
    SideEffect {
        if (model != null && applied[0] < block.messages.size) applied[0] = model.applyBlock(block.messages, surfaceId, applied[0])
    }
    return model
}

private val SURFACE_MESSAGES = listOf("createSurface", "updateComponents", "updateDataModel", "deleteSurface")

/** The surface an A2UI message targets. */
private fun JsonObject.targetSurface(): String? = SURFACE_MESSAGES.firstNotNullOfOrNull { key ->
    ((this[key] as? JsonObject)?.get("surfaceId") as? JsonPrimitive)?.content
}

/** Apply [messages] from [from] that target [surfaceId]; returns the new mark. */
private fun SurfaceModel.applyBlock(messages: List<JsonObject>, surfaceId: String, from: Int): Int {
    for (i in from until messages.size) {
        val m = messages[i]
        if (m.targetSurface() == surfaceId) runCatching { apply(m.toString()) }
    }
    return messages.size
}
