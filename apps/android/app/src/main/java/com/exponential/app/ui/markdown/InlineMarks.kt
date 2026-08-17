package com.exponential.app.ui.markdown

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark

/**
 * Builds a styled [AnnotatedString] from a plain line plus the inline marks that
 * fall within it (coordinates already line-local). Used by the read-only
 * renderer and the editor's visual transformation so both apply identical inline
 * styling. Link ranges also carry a `"link"` string annotation (the href) so
 * callers can wire taps.
 */
object InlineMarks {

    const val LINK_TAG = "link"

    fun annotate(text: String, marks: List<InlineMark>): AnnotatedString {
        if (text.isEmpty()) return AnnotatedString("")
        if (marks.isEmpty()) return AnnotatedString(text)

        return buildAnnotatedString {
            append(text)
            addStyles(this, text.length, marks)
        }
    }

    /**
     * The style half of [annotate], applied onto an existing builder whose text
     * is already appended — lets the editor's visual transformation stack line
     * styles, mark styles and chip styles in one build (EXP-534).
     */
    fun addStyles(builder: AnnotatedString.Builder, textLength: Int, marks: List<InlineMark>) {
        for (m in marks) {
            val start = m.start.coerceIn(0, textLength)
            val end = m.end.coerceIn(start, textLength)
            if (end <= start) continue
            when (m.kind) {
                InlineKind.Bold -> builder.addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, end)
                InlineKind.Italic -> builder.addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
                InlineKind.Strikethrough ->
                    builder.addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
                InlineKind.InlineCode -> builder.addStyle(
                    SpanStyle(fontFamily = FontFamily.Monospace, background = MdStyle.InlineCodeBg),
                    start, end,
                )
                InlineKind.Link -> {
                    builder.addStyle(SpanStyle(color = MdStyle.Link), start, end)
                    m.href?.let { builder.addStringAnnotation(LINK_TAG, it, start, end) }
                }
            }
        }
    }

    /** Convenience for an [AnnotatedString] over a whole line with no extra base style. */
    fun withStyleOf(base: SpanStyle, text: String, marks: List<InlineMark>): AnnotatedString =
        buildAnnotatedString {
            withStyle(base) { append(annotate(text, marks)) }
        }
}
