package at.exponential.ui.measure

import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.HighlightSpec
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.highlightCount
import at.exponential.ui.primitives.highlightSpecOf
import at.exponential.ui.primitives.markdownHighlightCount

/**
 * #EDE-95 (`fixtures/text-highlight.json`): the `highlight` prop as a spec:
 * an object with a string `query` that folds to something (else null), an
 * optional `current` (floored) and a non-empty `before`. The rule itself
 * (fold, ranges, the local current, a Markdown's searched blocks) lives in
 * the primitives' `TextHighlight.kt`, a port of src/highlight.ts.
 */
fun highlightSpec(value: JsonValue?): HighlightSpec? {
    val v = value?.obj ?: return null
    val current = when (val c = v["current"]) {
        is JsonValue.Num -> c.v
        is JsonValue.Str -> c.v.trim().toDoubleOrNull()
        else -> null
    }
    return highlightSpecOf(v["query"]?.string, current, v["before"]?.string)
}

/** A node's matches for its own `highlight` (0 without one): Text = its text, Markdown and MessageBubble = the markdown, Row = title + subtitle. */
fun nodeHighlightCount(component: String, props: Map<String, JsonValue>): Int {
    val spec = highlightSpec(props["highlight"]) ?: return 0
    fun str(key: String) = props[key]?.string.orEmpty()
    return when (component) {
        "Text" -> highlightCount(str("text"), spec.query)
        "Markdown", "MessageBubble" -> markdownHighlightCount(Markdown.parse(str("text")), spec.query)
        "Row" -> highlightCount(str("title"), spec.query) + highlightCount(str("subtitle"), spec.query)
        else -> 0
    }
}
