package com.exponential.app.ui.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

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
    val CodeBlockBg = Color.White.copy(alpha = 0.06f)
    val Blockquote = Color.White.copy(alpha = 0.6f)
    val Placeholder = Color.White.copy(alpha = 0.3f)
    val Dim = Color.White.copy(alpha = 0.3f) // thematic break

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

    // List indentation (iOS headIndent = depth*20 + 24).
    val listIndentBase = 24.dp
    val listIndentPerDepth = 20.dp

    val textInsetV = 4.dp
    val blockSpacing = 8.dp
}
