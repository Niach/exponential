package com.exponential.app.ui.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.theme.DesignTokens

/**
 * Visual tokens for the block markdown editor / viewer — a Compose port of iOS
 * `MarkdownStyle` (`apps/ios/Exponential/UI/Markdown/MarkdownAttributes.swift`)
 * so the two clients render identically. Body text is pinned at 17 sp (iOS
 * `.body`); colors are white-alpha tiers over the dark glass background.
 */
object MdStyle {
    val Text = Color.White.copy(alpha = 0.9f)
    val Link = Color(red = 0.42f, green = 0.64f, blue = 1.0f) // ~#6BA3FF
    val InlineCodeBg = Color.White.copy(alpha = 0.08f)
    val IssueRefBg = Color.White.copy(alpha = 0.10f) // #IDENTIFIER pill (web .issue-ref-pill)
    val IssueRefBorder = Color.White.copy(alpha = 0.16f) // its hairline (EXP-423, Linear parity)
    val ChipToken = Color.White.copy(alpha = 0.55f) // the muted `#IDENT` inside a chip
    val CodeBlockBg = Color.White.copy(alpha = 0.06f)
    val Blockquote = Color.White.copy(alpha = 0.6f)
    val QuoteBar = Color.White.copy(alpha = 0.2f) // blockquote left rule (EXP-246)
    val Placeholder = Color.White.copy(alpha = 0.3f)
    val Dim = Color.White.copy(alpha = 0.3f) // thematic break

    // GFM table chrome (EXP-726): a hairline grid on a barely-there header tint,
    // the same two tiers iOS uses for `tableBorder`/`tableHeaderBackground`.
    val TableBorder = Color.White.copy(alpha = 0.16f)
    val TableHeaderBg = Color.White.copy(alpha = 0.06f)

    /**
     * Inline-code chrome, the one thing a rendering surface may re-tint
     * (EXP-698). The document renderers — issue descriptions, comments, the
     * editor's own transformation — keep [Default]: monospace on a flat white
     * wash, byte-identical to what they have always drawn. A CHAT feed takes
     * [Chat], the tinted `code` treatment web and the desktop give agent
     * output, so a `--flag` or a `Foo.kt` reads as code at a glance in a wall
     * of narration.
     *
     * There is deliberately no stroke: a Compose [SpanStyle] can paint a
     * background but not a border, so a hairline round an inline run means
     * measuring the run and drawing behind the text (what `#IDENTIFIER` chips
     * do, and they cost a `TextLayoutResult` per line). The tinted fill plus
     * the tinted glyph carries it; `DesignTokens.Semantic.CodeStroke` stays
     * for the clients whose text stacks can express it.
     */
    data class InlineCodeStyle(
        val inlineCodeColor: Color? = null,
        val inlineCodeBg: Color = InlineCodeBg,
    )

    /** Document surfaces: no tint, the flat white wash. */
    val Default = InlineCodeStyle()

    /** Chat surfaces (the agent steering feed): the shared code accent. */
    val Chat = InlineCodeStyle(
        inlineCodeColor = DesignTokens.Semantic.CodeText,
        inlineCodeBg = DesignTokens.Semantic.CodeFill,
    )

    val bodySize = 17.sp
    val lineHeight = 25.sp

    val body = TextStyle(color = Text, fontSize = bodySize, lineHeight = lineHeight)

    fun heading(level: Int): TextStyle {
        val size = when (level) {
            1 -> 24.sp
            2 -> 20.sp
            3 -> 18.sp
            4 -> 16.sp
            5 -> 15.sp
            6 -> 14.sp
            else -> bodySize
        }
        return TextStyle(color = Text, fontWeight = FontWeight.SemiBold, fontSize = size, lineHeight = size * 1.25f)
    }

    val mono = body.copy(fontFamily = FontFamily.Monospace, fontSize = bodySize * 0.9f)

    // List indentation (iOS headIndent = depth*20 + 24). The glyph (bullet /
    // number / checkbox) occupies a [listGlyphWidth] column starting at the
    // indent; the text begins after it (EXP-534: as a TextIndent inside the
    // run's one multi-line field, with the glyph painted into the margin).
    val listIndentBase = 24.dp
    val listIndentPerDepth = 20.dp
    val listGlyphWidth = 24.dp

    // Code block left inset (the old per-row decoration's horizontal padding).
    val codeInsetX = 8.dp

    // Blockquote geometry (EXP-246, Linear-style): a vertical left bar with the
    // quoted text indented beside it.
    val quoteBarWidth = 3.dp
    val quoteIndent = 10.dp

    val textInsetV = 4.dp
    val blockSpacing = 8.dp

    // Table geometry (EXP-726). A column is as wide as its widest cell, capped
    // so one essay-length cell cannot push the rest off the horizontal scroll.
    val tableCellMinWidth = 56.dp
    val tableCellMaxWidth = 280.dp
    val tableCellPadX = 8.dp
    val tableCellPadY = 6.dp
    val tableBorderWidth = 1.dp

    // `#IDENTIFIER` chip geometry (EXP-423): a rounded RECT (iOS ~5pt, web 6px,
    // desktop 4) — mention pills stay unpainted here, they keep their flat span.
    val chipCornerRadius = 5.dp
    val chipPadX = 2.dp
    val chipIconSize = 13.dp
    // The status glyph sits in the hidden `#` cell, LEFT-anchored; this extra
    // advance on that one character is what keeps the identifier clear of the
    // 13dp art (EXP-655) — a monospace `#` alone is ~0.6em, narrower than the
    // icon. Relative so headings scale it with their font.
    val chipHashLetterSpacing = 0.45.em
}
