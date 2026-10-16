package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.AnsiCode
import com.exponential.app.domain.CodeToken
import com.exponential.app.domain.TraceSpec
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import kotlinx.coroutines.delay

// EXP-1324: THE CodeBlock — shadcn's code-block (radix-nova) at size `sm`,
// rebuilt from `fixtures/trace.json` `geometry.codeBlock`: tool output inside
// a Trace line's content. A radius-`lg` box with an input-colour hairline over
// the input colour at `fillAlpha`; the `Output` header (sans, medium,
// foreground) with the ghost copy button (ui-copy → ui-check for 2s) pulled
// `headerActionsOutset` into the padding; the code in mono, scrolling
// sideways (never wrapping) and at most `maxLines` tall. `ansi` colours command
// output (the escapes never reach the text, copy copies what shows);
// `destructive` = a failed call's block: destructive hairline, title and code,
// no fill.

enum class CodeBlockLanguage { Plain, Ansi }

/** The catalog's `codeToken` kinds this block paints (web `CODE_TOKEN_CLASS`:
 *  success = `--hljs-addition`, warning = `--hljs-number`, comment =
 *  muted-foreground, error = destructive; dark values). */
internal fun codeTokenColor(kind: String): Color? = when (kind) {
    "success" -> Color(0xFF67BB6B)
    "warning", "number" -> Color(0xFFF2943C)
    "comment", "operator", "punctuation" -> DesignTokens.Palette.MutedForeground
    "error" -> DesignTokens.Palette.Destructive
    else -> null
}

private fun annotate(lines: List<List<CodeToken>>, colored: Boolean): AnnotatedString = buildAnnotatedString {
    lines.forEachIndexed { index, tokens ->
        if (index > 0) append('\n')
        for (token in tokens) {
            val color = if (colored) codeTokenColor(token.kind) else null
            if (color == null) append(token.text) else withStyle(SpanStyle(color = color)) { append(token.text) }
        }
    }
}

@Composable
fun CodeBlock(
    code: String,
    modifier: Modifier = Modifier,
    title: String? = TraceSpec.OUTPUT,
    language: CodeBlockLanguage = CodeBlockLanguage.Plain,
    destructive: Boolean = false,
    /** Start scrolled to the end (tool output: the verdict is the last line). */
    scrollToEnd: Boolean = false,
) {
    val spec = TraceSpec.CodeBlock
    val shape = RoundedCornerShape(DesignTokens.Radius.Lg)
    val ink = if (destructive) DesignTokens.Palette.Destructive else DesignTokens.Palette.Foreground
    val trimmed = remember(code) { code.removeSuffix("\n") }
    val shown = remember(trimmed, language) {
        when (language) {
            CodeBlockLanguage.Ansi -> AnsiCode.tokenize(trimmed)
            // The same line-end normalisation as the ansi path (`AnsiCode.normalize`):
            // CRLF, then a lone CR (a progress bar's carriage returns).
            CodeBlockLanguage.Plain -> trimmed.replace("\r\n", "\n").replace('\r', '\n').split('\n')
                .map { listOf(CodeToken("plain", it)) }
        }
    }
    val text = remember(shown, destructive) { annotate(shown, colored = !destructive) }
    val copyText = remember(shown) { shown.joinToString("\n") { line -> line.joinToString("") { it.text } } }
    val codeStyle = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontWeight = FontWeight.Normal,
        fontSize = spec.FONT_SIZE.sp,
        lineHeight = spec.LINE_HEIGHT.sp,
        letterSpacing = 0.sp,
        lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
    )
    val maxCodeHeight = with(LocalDensity.current) { (spec.LINE_HEIGHT * spec.MAX_LINES).sp.toDp() }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .then(
                if (destructive) {
                    Modifier.border(
                        spec.BORDER.dp,
                        DesignTokens.Palette.Destructive.copy(alpha = spec.DESTRUCTIVE_BORDER_ALPHA),
                        shape,
                    )
                } else {
                    Modifier
                        .background(DesignTokens.Palette.Input.copy(alpha = DesignTokens.Palette.Input.alpha * spec.FILL_ALPHA))
                        .border(spec.BORDER.dp, DesignTokens.Palette.Input, shape)
                },
            ),
    ) {
        if (title != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = spec.PADDING.dp, end = spec.PADDING.dp, top = spec.PADDING.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    title,
                    style = TextStyle(
                        fontFamily = FontFamily.Default,
                        fontWeight = FontWeight.Medium,
                        fontSize = spec.TITLE_SIZE.sp,
                        lineHeight = 16.sp,
                        letterSpacing = 0.sp,
                    ),
                    color = ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                // The actions sit `headerActionsOutset` into the right/top
                // padding (shadcn's -my-1 -me-1), so the row stays the
                // button's height minus the outsets.
                Box(
                    modifier = Modifier
                        .offset(x = spec.HEADER_ACTIONS_OUTSET.dp)
                        .padding(vertical = 0.dp)
                        .size(spec.COPY_BUTTON.dp - (spec.HEADER_ACTIONS_OUTSET * 2).dp, spec.COPY_BUTTON.dp - (spec.HEADER_ACTIONS_OUTSET * 2).dp),
                    contentAlignment = Alignment.Center,
                ) {
                    CodeBlockCopyButton(copyText, tint = if (destructive) ink else DesignTokens.Palette.MutedForeground)
                }
            }
        }
        val vertical = rememberScrollState()
        val horizontal = rememberScrollState()
        if (scrollToEnd) {
            LaunchedEffect(text) { vertical.scrollTo(vertical.maxValue) }
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = maxCodeHeight + (spec.PADDING + if (title != null) spec.CONTENT_TOP_WITH_HEADER else spec.PADDING).dp)
                .verticalScroll(vertical)
                .horizontalScroll(horizontal)
                .padding(
                    start = spec.PADDING.dp,
                    end = spec.PADDING.dp,
                    top = if (title != null) spec.CONTENT_TOP_WITH_HEADER.dp else spec.PADDING.dp,
                    bottom = spec.PADDING.dp,
                ),
        ) {
            Text(text, style = codeStyle, color = ink, softWrap = false)
        }
    }
}

/** The ghost icon-xs copy button: ui-copy → ui-check for 2s. */
@Composable
private fun CodeBlockCopyButton(value: String, tint: Color) {
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            delay(2_000)
            copied = false
        }
    }
    val spec = TraceSpec.CodeBlock
    IconButton(
        onClick = {
            clipboard.setText(AnnotatedString(value))
            copied = true
        },
        modifier = Modifier
            .size(spec.COPY_BUTTON.dp)
            .clip(CircleShape)
            .semantics { contentDescription = if (copied) TraceSpec.COPIED else TraceSpec.COPY }
            .testTag("code-block-copy"),
    ) {
        Icon(
            if (copied) ExpIcons.uiCheck else ExpIcons.uiCopy,
            contentDescription = null,
            modifier = Modifier.size(spec.COPY_ICON.dp),
            tint = tint,
        )
    }
}
