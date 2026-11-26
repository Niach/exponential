package at.exponential.ui.measure

import at.exponential.ui.json.JsonValue
import kotlin.math.roundToInt

/**
 * Round 4 group B (`fixtures/text-runs.json`): a Text's `runs` and its
 * `dropCap`, the rule the TS reference (src/text-runs.ts) and the Rust core
 * (`eui_core::text_runs`) implement. The core hands the painter the NORMAL
 * form (`props.runs`, `props.text` = the joined text, `props.dropCap` =
 * lines); this twin replays the fixture and splits the drop cap off.
 */
data class TextRun(val text: String, val weight: String? = null, val tone: String? = null, val mark: String? = null, val link: String? = null) {
    /** `$type.weight.<w>` (null = the Text's own). */
    val fontWeight: Int? get() = when (weight) {
        null -> null
        "medium" -> 500
        "semibold" -> 600
        "bold" -> 700
        else -> 400
    }

    fun toJson(): JsonValue = JsonValue.Obj(
        buildMap {
            put("text", JsonValue.Str(text))
            weight?.let { put("weight", JsonValue.Str(it)) }
            tone?.let { put("tone", JsonValue.Str(it)) }
            mark?.let { put("mark", JsonValue.Str(it)) }
            link?.let { put("link", JsonValue.Str(it)) }
        },
    )

    internal fun same(o: TextRun) = weight == o.weight && tone == o.tone && mark == o.mark && link == o.link
}

object TextRuns {
    val WEIGHTS = listOf("regular", "medium", "semibold", "bold")
    val MARKS = listOf("highlight", "code", "strike", "underline")
    val TONES = listOf("neutral", "primary", "success", "warning", "danger", "info")
    const val MIN = 2
    const val MAX = 4
    const val LINES = 3
    const val RATIO = 1.375f
    /** `$spacing.xs`: the gap after the cap. */
    const val GAP_TOKEN = "xs"

    private fun runText(v: JsonValue?): String? = when (v) {
        is JsonValue.Str -> v.v
        is JsonValue.Num, is JsonValue.Bool -> v.displayText
        else -> null
    }

    /** `runs` normalised (null = not an array: the Text shows `text`). */
    fun normalise(value: JsonValue?): List<TextRun>? {
        val arr = value?.array ?: return null
        val out = ArrayList<TextRun>()
        for (entry in arr) {
            val run = when (entry) {
                is JsonValue.Str -> TextRun(entry.v)
                is JsonValue.Obj -> runText(entry["text"])?.let { text ->
                    TextRun(
                        text,
                        weight = entry["weight"]?.string?.takeIf { it in WEIGHTS },
                        tone = entry["tone"]?.string?.takeIf { it in TONES },
                        mark = entry["mark"]?.string?.takeIf { it in MARKS },
                        link = entry["link"]?.string?.takeIf { it.isNotEmpty() },
                    )
                }
                else -> null
            } ?: continue
            if (run.text.isEmpty()) continue
            val last = out.lastOrNull()
            if (last != null && last.same(run)) out[out.size - 1] = last.copy(text = last.text + run.text) else out.add(run)
        }
        return out
    }

    fun joined(runs: List<TextRun>): String = runs.joinToString("") { it.text }

    /** `dropCap` as lines (0 = none) for a Text's props. */
    fun dropCapLines(props: Map<String, JsonValue>): Int {
        if (props["shimmer"]?.let { it is JsonValue.Bool && it.v } == true) return 0
        val truncate = props["truncate"]?.string
        if (truncate == "end" || truncate == "middle") return 0
        if (((props["lines"] as? JsonValue.Num)?.v ?: 0.0) > 0.0) return 0
        return when (val v = props["dropCap"]) {
            is JsonValue.Bool -> if (v.v) LINES else 0
            is JsonValue.Num -> if (!v.v.isFinite()) 0 else Math.round(v.v).toInt().let { if (it < MIN) 0 else minOf(it, MAX) }
            else -> 0
        }
    }

    private fun isJoiner(cp: Int): Boolean =
        cp in 0x300..0x36f || cp in 0x1ab0..0x1aff || cp in 0x1dc0..0x1dff || cp in 0x20d0..0x20ff || cp in 0xfe20..0xfe2f || cp == 0xfe0e || cp == 0xfe0f

    /** The drop cap off the runs: leading ASCII spaces, tabs and line breaks drop; the cap = the first scalar plus the combining marks and variation selectors after it, in the weight and tone of its run; null = no letter. */
    fun dropCapSplit(runs: List<TextRun>): Pair<TextRun, List<TextRun>>? {
        val rest = runs.toMutableList()
        while (rest.isNotEmpty()) {
            val t = rest[0].text.trimStart(' ', '\t', '\n', '\r')
            if (t.isNotEmpty()) {
                rest[0] = rest[0].copy(text = t)
                break
            }
            rest.removeAt(0)
        }
        val first = rest.firstOrNull() ?: return null
        val s = first.text
        var end = Character.charCount(s.codePointAt(0))
        while (end < s.length) {
            val cp = s.codePointAt(end)
            if (!isJoiner(cp)) break
            end += Character.charCount(cp)
        }
        val cap = TextRun(s.substring(0, end), weight = first.weight, tone = first.tone)
        val tail = s.substring(end)
        if (tail.isEmpty()) rest.removeAt(0) else rest[0] = first.copy(text = tail)
        return cap to rest
    }

    /** The cap's font size: round(fontSize + (lines − 1) × lineHeight × 1.375). */
    fun dropCapSize(fontSize: Float, lineHeight: Float, lines: Int): Float =
        (fontSize + (lines - 1) * lineHeight * RATIO).let { Math.round(it.toDouble()).toFloat() }

    /** The normal form the core put in `props.runs` (null = none). */
    fun fromProps(props: Map<String, JsonValue>): List<TextRun>? = props["runs"]?.array?.mapNotNull { r ->
        val o = r.obj ?: return@mapNotNull null
        TextRun(o["text"]?.string ?: return@mapNotNull null, o["weight"]?.string, o["tone"]?.string, o["mark"]?.string, o["link"]?.string)
    }
}

/** What a Text paints through the rich path (null = its plain text): the spans after the cap, the cap and its lines. */
class RichSpans(val runs: List<TextRun>, val cap: TextRun?, val lines: Int)

fun richSpans(props: Map<String, JsonValue>, transform: (String) -> String): RichSpans? {
    val lines = (props["dropCap"] as? JsonValue.Num)?.v?.roundToInt() ?: 0
    val runs = TextRuns.fromProps(props)
    if (props["shimmer"]?.let { it is JsonValue.Bool && it.v } == true || (runs == null && lines == 0)) return null
    val all = (runs ?: props["text"]?.displayText?.takeIf { it.isNotEmpty() }?.let { listOf(TextRun(it)) } ?: emptyList()).map { it.copy(text = transform(it.text)) }
    if (lines > 0) TextRuns.dropCapSplit(all)?.let { (cap, rest) -> return RichSpans(rest, cap, lines) }
    return RichSpans(all, null, 0)
}
