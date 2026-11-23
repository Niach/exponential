package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * #EXP-2067 / #EXP-2073 (fixtures/chat-express.json): Express blocks inline
 * in an agent's text ×4. The Kotlin mirror of the TS reference
 * `packages/domain-contract/src/chat-express.ts` (desktop
 * `domain::chat_express`, ExpCore `ChatExpress`).
 *
 * The SPLIT and the COMPILE are the eui SDK's (`expressUnwrapJson` + an
 * unwrapped `ExpressDecoder` per block, [FfiSdk] in the app), injected as an
 * [Sdk] so this stays a pure rule. Everything that reads an agent text
 * WITHOUT drawing it (previews, find, read-aloud, copy) uses [plainProse].
 */
object ChatExpress {
    /** `limits` (locked against the fixture by the test). */
    object Limits {
        const val BLOCKS_PER_ROW_MAX = 8
        const val BLOCK_MAX_BYTES = 32768
        const val BLOCK_MAX_SURFACES = 4
        const val BLOCK_MAX_COMPONENTS = 200
        const val ACTION_CONTEXT_MAX_BYTES = 4096
        const val ACTION_NAME_MAX = 64
        const val NOTE_LINE_MAX = 120
    }

    /** `copy`. */
    object Copy {
        const val BLOCK_FAILED = "This view could not be drawn"
        const val BLOCK_ONLY = "Sent a view"
        const val ACTION_MARKER = "{author} pressed {name}"
        const val YOU = "You"
        const val ACTION_TOO_LARGE = "That action carries too much data to send"
    }

    /** `layout.blockGap` (dp). */
    const val BLOCK_GAP = 8

    /** `replyEvent`. */
    const val REPLY_EVENT = "reply"

    /** THE `action` message card's kind (db-schema `ActionMessageCard`). */
    const val ACTION_CARD_KIND = "action"

    /** `gate`. */
    const val GATE = "^[ \\t]*<a2ui(?:[ \\t][^>\\n]*)?>"
    private val gateRegex = Regex(GATE, RegexOption.MULTILINE)
    private val openLine = Regex(GATE)
    private val fence = Regex("^[ \\t]*(```|~~~)[\\w-]*[ \\t]*$")
    private val closeLine = Regex("^[ \\t]*</a2ui>[ \\t]*$")
    private const val TAG = "<a2ui>"
    private const val UNCLOSED_MESSAGE = "The block has no </a2ui> line."
    private const val ACTION_LEAD = "[action] "

    // ── The SDK, injected ────────────────────────────────────────────────

    /** One part of `unwrap`. */
    sealed interface RawPart {
        data class Text(val text: String) : RawPart
        data class Block(val raw: String, val isFinal: Boolean) : RawPart
    }

    data class Issue(val at: Int, val message: String)

    /** What a block's decoder returns per push (the SDK's `Decoded`). */
    data class Decoded(val messages: List<JsonObject>, val issues: List<Issue>)

    interface Decoder {
        fun push(chunk: String): Decoded
        fun end(): Decoded
    }

    interface Sdk {
        fun unwrap(text: String): List<RawPart>

        /** An UNWRAPPED Express stream whose default surface id is [surfaceId]. */
        fun decoder(surfaceId: String): Decoder
    }

    /** The FFI's JSON shapes (`express_unwrap_json`, the decoders' `{messages, issues}`). */
    fun parseUnwrap(json: String): List<RawPart> =
        (runCatching { Json.parseToJsonElement(json) }.getOrNull() as? JsonArray).orEmpty().mapNotNull { el ->
            val o = el as? JsonObject ?: return@mapNotNull null
            val text = o.str("text")
            val raw = o.str("a2ui_raw")
            when {
                raw != null -> RawPart.Block(raw, (o["is_final"] as? JsonPrimitive)?.content != "false")
                text != null -> RawPart.Text(text)
                else -> null
            }
        }

    fun parseDecoded(json: String): Decoded {
        val o = runCatching { Json.parseToJsonElement(json) as? JsonObject }.getOrNull() ?: return Decoded(emptyList(), emptyList())
        val messages = (o["messages"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
        val issues = (o["issues"] as? JsonArray).orEmpty().mapNotNull { el ->
            val i = el as? JsonObject ?: return@mapNotNull null
            Issue((i["at"] as? JsonPrimitive)?.content?.toIntOrNull() ?: 0, i.str("message").orEmpty())
        }
        return Decoded(messages, issues)
    }

    // ── Segments ─────────────────────────────────────────────────────────

    enum class BlockStatus(val wire: String) { Open("open"), Ok("ok"), Failed("failed"), TooMany("tooMany"), TooLarge("tooLarge") }

    /** `segmentCases` without the compile. */
    sealed interface Segment {
        data class Prose(val text: String) : Segment
        data class Block(val key: String, val raw: String, val closed: Boolean) : Segment
    }

    sealed interface RowSegment {
        data class Prose(val text: String) : RowSegment

        data class Block(
            val key: String,
            val raw: String,
            val closed: Boolean,
            val status: BlockStatus,
            /** The SDK's category or `Unclosed`; only on `failed`. */
            val error: String? = null,
            /** The SDK's own words (never fixture-locked). */
            val errorMessage: String? = null,
            /** Surface ids its stream created and did not delete. */
            val surfaces: List<String>,
            /** EVERY A2UI message of the block's stream since it (re)started. */
            val messages: List<JsonObject>,
            /** A restart bumps it (the renderer starts over). */
            val generation: Int,
        ) : RowSegment {
            /** Drawn with THE inline marker under it. */
            val marked: Boolean get() = status == BlockStatus.Failed || status == BlockStatus.TooMany || status == BlockStatus.TooLarge
        }
    }

    /** A row's text holds blocks: one of its lines starts with the open tag. */
    fun hasExpressBlock(text: String): Boolean = gateRegex.containsMatchIn(text)

    /** While a row streams, a last line that is a proper prefix of `<a2ui>` is held back. */
    fun withoutHeldTail(text: String, final: Boolean): String {
        if (final) return text
        val nl = text.lastIndexOf('\n')
        val last = text.substring(nl + 1).trimStart()
        if (last.isNotEmpty() && last.length < TAG.length && TAG.startsWith(last)) return text.substring(0, nl + 1).removeSuffix("\n")
        return text
    }

    /** The key of a synced row (`m:<messages.id>`). */
    fun syncedRowKey(messageId: String): String = "m:$messageId"

    /** The key of a feed narration row: its messageId, else its first seq. */
    fun feedRowKey(sessionId: String, messageId: String?, firstSeq: Long): String =
        if (!messageId.isNullOrEmpty()) "n:$sessionId:$messageId" else "e:$sessionId:$firstSeq"

    /** `segmentCases` without the compile: null when the gate fails. */
    fun expressSegments(text: String, rowKey: String, sdk: Sdk): List<Segment>? {
        if (!hasExpressBlock(text)) return null
        var n = 0
        val out = ArrayList<Segment>()
        for (part in sdk.unwrap(text)) {
            when (part) {
                is RawPart.Text -> if (part.text.isNotBlank()) out.add(Segment.Prose(part.text))
                is RawPart.Block -> {
                    n += 1
                    out.add(Segment.Block("$rowKey#$n", part.raw, part.isFinal))
                }
            }
        }
        return out
    }

    private val issuePrefix = listOf(
        "parse error: " to "ParseError",
        "validation error: " to "ValidationError",
        "catalog error: " to "CatalogError",
    )

    /** A decoder issue's message → the category and the SDK's words. */
    fun issueCategory(message: String): Pair<String, String> {
        for ((prefix, category) in issuePrefix) {
            if (message.startsWith(prefix)) return category to message.substring(prefix.length)
        }
        return "ParseError" to message
    }

    private fun utf8(s: String): Int = s.toByteArray(Charsets.UTF_8).size

    private data class Tally(val surfaces: List<String>, val components: Int)

    private fun tally(messages: List<JsonObject>): Tally {
        val surfaces = ArrayList<String>()
        val comps = LinkedHashMap<String, MutableSet<String>>()
        for (m in messages) {
            val create = m["createSurface"] as? JsonObject
            val update = m["updateComponents"] as? JsonObject
            if (create != null) {
                val id = create.str("surfaceId").orEmpty()
                surfaces.add(id)
                comps[id] = LinkedHashSet()
            }
            val c = create ?: update
            if (c != null) {
                val set = comps[c.str("surfaceId").orEmpty()]
                (c["components"] as? JsonArray)?.forEach { k -> (k as? JsonObject)?.str("id")?.let { set?.add(it) } }
            }
            val del = m["deleteSurface"] as? JsonObject
            if (del != null) {
                val id = del.str("surfaceId").orEmpty()
                surfaces.remove(id)
                comps.remove(id)
            }
        }
        return Tally(surfaces, comps.values.sumOf { it.size })
    }

    private class LiveBlock(
        var raw: String,
        val decoder: Decoder,
        val messages: MutableList<JsonObject>,
        var issue: Issue?,
        var ended: Boolean,
        val generation: Int,
    )

    /**
     * One row's blocks across its updates (`streamCases`): [update] re-splits
     * the text, pushes only a block's new suffix, restarts a block whose raw
     * changed otherwise, and ends a block once it is closed or the row final.
     */
    class ExpressRow(private val rowKey: String, private val sdk: Sdk) {
        private val blocks = LinkedHashMap<String, LiveBlock>()

        fun update(text: String, final: Boolean): List<RowSegment>? {
            val shown = withoutHeldTail(text, final)
            val segments = expressSegments(shown, rowKey, sdk)
            if (segments == null) {
                blocks.clear()
                if (shown == text) return null
                // a held tail: the prose before it, the tag still arriving
                return if (shown.isNotBlank()) listOf(RowSegment.Prose(shown.trim())) else emptyList()
            }
            var index = 0
            val seen = HashSet<String>()
            val out = segments.map { seg ->
                when (seg) {
                    is Segment.Prose -> RowSegment.Prose(seg.text)
                    is Segment.Block -> {
                        seen.add(seg.key)
                        block(seg, index++, final)
                    }
                }
            }
            blocks.keys.retainAll(seen)
            return out
        }

        private fun block(seg: Segment.Block, index: Int, final: Boolean): RowSegment.Block {
            fun base(status: BlockStatus, generation: Int) =
                RowSegment.Block(seg.key, seg.raw, seg.closed, status, surfaces = emptyList(), messages = emptyList(), generation = generation)
            if (index >= Limits.BLOCKS_PER_ROW_MAX) return base(BlockStatus.TooMany, 0)
            if (utf8(seg.raw) > Limits.BLOCK_MAX_BYTES) return base(BlockStatus.TooLarge, 0)
            var live = blocks[seg.key]
            if (live != null && !(seg.raw.startsWith(live.raw) && !(live.ended && seg.raw != live.raw))) {
                live = fresh(seg.key, live.generation + 1)
            }
            if (live == null) live = fresh(seg.key, 0)
            blocks[seg.key] = live
            if (!live.ended && seg.raw.length > live.raw.length) {
                take(live, live.decoder.push(seg.raw.substring(live.raw.length)))
                live.raw = seg.raw
            }
            if (!live.ended && (seg.closed || final)) {
                take(live, live.decoder.end())
                live.ended = true
            }
            val t = tally(live.messages)
            if (t.surfaces.size > Limits.BLOCK_MAX_SURFACES || t.components > Limits.BLOCK_MAX_COMPONENTS) {
                return base(BlockStatus.TooLarge, live.generation)
            }
            val messages = live.messages.toList()
            fun with(status: BlockStatus, error: String? = null, message: String? = null) =
                RowSegment.Block(seg.key, seg.raw, seg.closed, status, error, message, t.surfaces, messages, live.generation)
            live.issue?.let { issue ->
                val (category, message) = issueCategory(issue.message)
                return with(BlockStatus.Failed, category, message)
            }
            if (!seg.closed) {
                return if (final) with(BlockStatus.Failed, "Unclosed", UNCLOSED_MESSAGE) else with(BlockStatus.Open)
            }
            return with(BlockStatus.Ok)
        }

        private fun fresh(key: String, generation: Int) =
            LiveBlock("", sdk.decoder(key), ArrayList(), null, false, generation)

        private fun take(live: LiveBlock, d: Decoded) {
            if (live.issue != null) return
            live.messages.addAll(d.messages)
            if (d.issues.isNotEmpty()) live.issue = d.issues[0]
        }
    }

    /** `segmentCases` in one go (a fresh row). */
    fun expressRow(text: String, rowKey: String, final: Boolean, sdk: Sdk): List<RowSegment>? =
        ExpressRow(rowKey, sdk).update(text, final)

    /** The drawn prose of a row: its prose segments joined by a blank line. */
    fun expressProse(segments: List<RowSegment>?, text: String): String {
        if (segments == null) return text
        return segments.filterIsInstance<RowSegment.Prose>().joinToString("\n\n") { it.text }
    }

    /** The PURE prose (`plainProseCases`) for every read that does not draw. */
    fun plainProse(text: String): String {
        if (!hasExpressBlock(text)) return text
        val lines = text.split("\n")
        val keep = ArrayList<String?>()
        var i = 0
        while (i < lines.size) {
            val line = lines[i]
            if (!openLine.containsMatchIn(line)) {
                keep.add(line)
                i++
                continue
            }
            // a fence line right before the block goes with it
            val prev = keep.lastOrNull()
            if (keep.isNotEmpty() && prev != null && fence.containsMatchIn(prev)) keep.removeAt(keep.lastIndex)
            val rest = line.replaceFirst(openLine, "")
            var j = i
            if (!rest.contains("</a2ui>")) {
                j = i + 1
                while (j < lines.size && !closeLine.containsMatchIn(lines[j])) j++
            }
            if (j + 1 < lines.size && fence.containsMatchIn(lines[j + 1])) j++
            keep.add(null)
            i = j + 1
        }
        val runs = ArrayList<String>()
        var run = ArrayList<String>()
        for (l in keep) {
            if (l == null) {
                runs.add(run.joinToString("\n"))
                run = ArrayList()
            } else {
                run.add(l)
            }
        }
        runs.add(run.joinToString("\n"))
        return runs.map { it.trim() }.filter { it.isNotEmpty() }.joinToString("\n\n")
    }

    // ── Actions ──────────────────────────────────────────────────────────

    /** THE `action` message card. */
    data class ActionCard(
        val name: String,
        val context: JsonObject?,
        val surfaceId: String,
        val block: String?,
        val messageId: String?,
    ) {
        /** The card's wire object (`{kind, name, context, surfaceId, block, messageId}`). */
        fun toJson(): JsonObject = buildJsonObject {
            put("kind", ACTION_CARD_KIND)
            put("name", name)
            put("context", context ?: JsonNull)
            put("surfaceId", surfaceId)
            put("block", block?.let(::JsonPrimitive) ?: JsonNull)
            put("messageId", messageId?.let(::JsonPrimitive) ?: JsonNull)
        }

        companion object {
            /** A stored card back (null = not an action card). */
            fun of(card: JsonObject?): ActionCard? {
                if (card == null || card.str("kind") != ACTION_CARD_KIND) return null
                val name = card.str("name") ?: return null
                return ActionCard(name, card["context"] as? JsonObject, card.str("surfaceId").orEmpty(), card.str("block"), card.str("messageId"))
            }
        }
    }

    data class ActionOrigin(val surfaceId: String, val block: String?, val messageId: String?)

    /** An action event as the renderer hands it over. */
    data class ActionEvent(val name: JsonElement?, val context: JsonElement?, val userMessage: JsonElement?)

    sealed interface ActionResult {
        data class Post(val body: String, val card: ActionCard) : ActionResult

        /** Nothing posted: THE failed-action toast ([Copy.ACTION_TOO_LARGE]). */
        data object TooLarge : ActionResult
    }

    private fun trimmed(v: JsonElement?): String = (v as? JsonPrimitive)?.takeIf { it.isString }?.content?.trim().orEmpty()

    /** `actionCases`: the person message an action posts, or [ActionResult.TooLarge]. */
    fun surfaceAction(event: ActionEvent, origin: ActionOrigin): ActionResult? {
        val name = (event.name as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() } ?: return null
        val context = event.context as? JsonObject
        if (name.codePointCount(0, name.length) > Limits.ACTION_NAME_MAX) return ActionResult.TooLarge
        if (context != null && utf8(context.toString()) > Limits.ACTION_CONTEXT_MAX_BYTES) return ActionResult.TooLarge
        val user = trimmed(event.userMessage)
        val body = if (name == REPLY_EVENT) user.ifEmpty { trimmed(context?.get("text")) } else user
        return ActionResult.Post(body, ActionCard(name, context, origin.surfaceId, origin.block, origin.messageId))
    }

    /** [surfaceAction] for a renderer's event. */
    fun surfaceAction(name: String, context: JsonObject?, userMessage: String?, origin: ActionOrigin): ActionResult? =
        surfaceAction(ActionEvent(JsonPrimitive(name), context, userMessage?.let(::JsonPrimitive)), origin)

    /** An exponential_show surface's origin (`show:<messages.id>`). */
    fun showActionOrigin(messageId: String): ActionOrigin = ActionOrigin("show:$messageId", null, messageId)

    /** `inputCases`: the ONE input the device hands the agent. */
    fun actionInput(card: ActionCard, body: String): String {
        val head = buildJsonObject {
            put("name", card.name)
            put("surfaceId", card.surfaceId)
            put("block", card.block?.let(::JsonPrimitive) ?: JsonNull)
            put("messageId", card.messageId?.let(::JsonPrimitive) ?: JsonNull)
            put("context", card.context ?: JsonNull)
        }
        return "$ACTION_LEAD$head${if (body.isNotEmpty()) "\n$body" else ""}"
    }

    data class ParsedInput(val card: ActionCard, val body: String)

    /** A feed input back to its card and body (null = not an action input). */
    fun parseActionInput(text: String): ParsedInput? {
        if (!text.startsWith(ACTION_LEAD)) return null
        val nl = text.indexOf('\n')
        val head = if (nl < 0) text.substring(ACTION_LEAD.length) else text.substring(ACTION_LEAD.length, nl)
        val v = runCatching { Json.parseToJsonElement(head) as? JsonObject }.getOrNull() ?: return null
        val name = v.str("name") ?: return null
        val surfaceId = v.str("surfaceId") ?: return null
        return ParsedInput(
            ActionCard(name, v["context"] as? JsonObject, surfaceId, v.str("block"), v.str("messageId")),
            if (nl < 0) "" else text.substring(nl + 1),
        )
    }

    /** The marker's words of an action row without a body. */
    fun actionMarker(name: String, author: String?): String =
        Copy.ACTION_MARKER.replace("{author}", author ?: Copy.YOU).replace("{name}", name)

    // ── Failed blocks tell the agent ─────────────────────────────────────

    private data class BlockError(val category: String, val message: String, val line: Int)

    /** `failureNoteCases`: the lines a finished turn's failed blocks leave (null = none). */
    fun failureNote(turnTexts: List<String>, sdk: Sdk): String? {
        val notes = ArrayList<String>()
        var n = 0
        for (text in turnTexts) {
            if (!hasExpressBlock(text)) continue
            var inRow = 0
            for (part in sdk.unwrap(text)) {
                if (part !is RawPart.Block) continue
                n += 1
                inRow += 1
                val raw = part.raw
                val err = when {
                    inRow > Limits.BLOCKS_PER_ROW_MAX -> BlockError("Limit", "More than ${Limits.BLOCKS_PER_ROW_MAX} blocks in one message.", 1)
                    utf8(raw) > Limits.BLOCK_MAX_BYTES -> BlockError("Limit", "More than ${Limits.BLOCK_MAX_BYTES} bytes in one block.", 1)
                    else -> blockError(raw, part.isFinal, "note#$n", sdk)
                } ?: continue
                var line = (raw.split("\n").getOrNull(err.line - 1) ?: "").trim()
                val cps = line.codePoints().toArray()
                if (cps.size > Limits.NOTE_LINE_MAX) line = String(cps, 0, Limits.NOTE_LINE_MAX - 1) + "…"
                notes.add("[view] Block $n of your last turn did not render (${err.category}): ${err.message} Line ${err.line}: $line")
            }
        }
        return notes.takeIf { it.isNotEmpty() }?.joinToString("\n")
    }

    private fun blockError(raw: String, closed: Boolean, key: String, sdk: Sdk): BlockError? {
        val lines = raw.split("\n")
        val decoder = sdk.decoder(key)
        val messages = ArrayList<JsonObject>()
        fun check(d: Decoded, line: Int): BlockError? {
            for (m in d.messages) {
                messages.add(m)
                val t = tally(messages)
                if (t.surfaces.size > Limits.BLOCK_MAX_SURFACES) return BlockError("Limit", "More than ${Limits.BLOCK_MAX_SURFACES} surfaces in one block.", line)
                if (t.components > Limits.BLOCK_MAX_COMPONENTS) return BlockError("Limit", "More than ${Limits.BLOCK_MAX_COMPONENTS} components in one block.", line)
            }
            val issue = d.issues.firstOrNull() ?: return null
            val (category, message) = issueCategory(issue.message)
            return BlockError(category, message, line)
        }
        for (i in lines.indices) {
            check(decoder.push(if (i < lines.size - 1) "${lines[i]}\n" else lines[i]), i + 1)?.let { return it }
        }
        check(decoder.end(), lines.size)?.let { return it }
        if (!closed) return BlockError("Unclosed", UNCLOSED_MESSAGE, lines.size)
        return null
    }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
}
