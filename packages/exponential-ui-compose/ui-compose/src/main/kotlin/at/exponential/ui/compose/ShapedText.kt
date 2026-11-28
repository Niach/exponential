package at.exponential.ui.compose

import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.primitives.MarkdownFontResolver

/**
 * `BasicText` of a surface's text through the shaper's `Inter Symbols`
 * fallback ([TextShaper.symbols]): a key symbol the face lacks paints in
 * the face the measurer shaped it with. Text without one is the plain
 * `BasicText(String)` it always was.
 */
@Composable
internal fun ShapedText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    maxLines: Int = Int.MAX_VALUE,
) {
    val symbols = LocalSurfaceModel.current.textShaper().symbols(text, style)
    if (symbols == null) {
        BasicText(text, modifier = modifier, style = style, onTextLayout = onTextLayout, overflow = overflow, softWrap = softWrap, maxLines = maxLines)
    } else {
        BasicText(symbols, modifier = modifier, style = style, onTextLayout = onTextLayout, overflow = overflow, softWrap = softWrap, maxLines = maxLines)
    }
}

/** [ShapedText] of an annotated string (its own spans' faces decide; the same instance when no symbol needs the fallback). */
@Composable
internal fun ShapedText(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    style: TextStyle = TextStyle.Default,
    onTextLayout: ((TextLayoutResult) -> Unit)? = null,
    overflow: TextOverflow = TextOverflow.Clip,
    softWrap: Boolean = true,
    maxLines: Int = Int.MAX_VALUE,
    inlineContent: Map<String, InlineTextContent> = emptyMap(),
) {
    BasicText(
        LocalSurfaceModel.current.textShaper().symbols(text, style),
        modifier = modifier,
        style = style,
        onTextLayout = onTextLayout,
        overflow = overflow,
        softWrap = softWrap,
        maxLines = maxLines,
        inlineContent = inlineContent,
    )
}

/**
 * The Markdown painter's fonts through [shaper]: its families (mono = the
 * theme's mono, else `ui-monospace`) and its `Inter Symbols` fallback, so
 * a painted run is the one `MarkdownShaper` measured.
 */
internal fun markdownFonts(shaper: TextShaper): MarkdownFontResolver = object : MarkdownFontResolver {
    override fun family(name: String?, mono: Boolean) = if (mono) shaper.family(name ?: "ui-monospace") else shaper.family(name)

    override fun fallback(text: AnnotatedString, style: TextStyle): AnnotatedString = shaper.symbols(text, style)
}
