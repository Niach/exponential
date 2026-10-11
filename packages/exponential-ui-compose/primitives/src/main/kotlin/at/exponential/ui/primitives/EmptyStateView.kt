package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The two empty-state sizes (EXP-1274, `packages/domain-contract/fixtures/
 * empty-state.json` `sizes`): [Page] = what a whole screen or pane says when
 * it has nothing (icon disc, title, optional description and action);
 * [List] = what a list inside a section, a sidebar or a picker says (a bare
 * glyph over ONE muted line, never a description or a button).
 */
enum class EmptyStateSize { Page, List }

/**
 * The empty state every client draws, centred. [EmptyStateSize.Page]: an
 * [icon] on an [iconDisc] circle of [iconDiscSize] (no disc when null), a
 * [title], a [description] line and one action: the [action] slot, or a
 * capsule button from [actionLabel] + [onAction], on a column of at most
 * 448dp with 48dp vertical padding. [EmptyStateSize.List]: the bare glyph
 * at 60% muted over ONE 14sp muted [title] line, 24dp vertical padding;
 * [description] and every action are ignored. Text styles and colours
 * default from the tokens.
 */
@Composable
fun EmptyStateView(
    title: String,
    modifier: Modifier = Modifier,
    size: EmptyStateSize = EmptyStateSize.Page,
    description: String? = null,
    icon: (@Composable () -> Unit)? = null,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
    action: (@Composable () -> Unit)? = null,
    iconDisc: Color? = LocalPrimitiveTokens.current.muted,
    iconDiscSize: Dp = 48.dp,
    iconColor: Color = when (size) {
        EmptyStateSize.Page -> LocalPrimitiveTokens.current.mutedForeground
        EmptyStateSize.List -> LocalPrimitiveTokens.current.mutedForeground.copy(alpha = 0.6f)
    },
    titleStyle: TextStyle = when (size) {
        EmptyStateSize.Page -> TextStyle(fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
        EmptyStateSize.List -> TextStyle(fontSize = 14.sp)
    },
    titleColor: Color = when (size) {
        EmptyStateSize.Page -> LocalPrimitiveTokens.current.foreground
        EmptyStateSize.List -> LocalPrimitiveTokens.current.mutedForeground
    },
    descriptionStyle: TextStyle = TextStyle(fontSize = 13.sp),
    descriptionColor: Color = LocalPrimitiveTokens.current.mutedForeground,
    descriptionPadding: Dp = 0.dp,
    spacing: Dp = when (size) {
        EmptyStateSize.Page -> 12.dp
        EmptyStateSize.List -> 6.dp
    },
    contentPadding: PaddingValues = when (size) {
        EmptyStateSize.Page -> PaddingValues(horizontal = 24.dp, vertical = 48.dp)
        EmptyStateSize.List -> PaddingValues(horizontal = 16.dp, vertical = 24.dp)
    },
) {
    val tokens = LocalPrimitiveTokens.current
    val page = size == EmptyStateSize.Page
    Box(modifier = modifier.padding(contentPadding), contentAlignment = Alignment.Center) {
        Column(
            modifier = if (page) Modifier.widthIn(max = 448.dp) else Modifier,
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(spacing),
        ) {
            if (icon != null) {
                CompositionLocalProvider(LocalContentColor provides iconColor) {
                    if (page && iconDisc != null) {
                        Box(
                            Modifier.size(iconDiscSize).background(iconDisc, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) { icon() }
                    } else {
                        icon()
                    }
                }
            }
            BasicText(
                title,
                style = titleStyle.merge(TextStyle(color = titleColor, textAlign = TextAlign.Center)),
            )
            if (!page) return@Column
            if (description != null) {
                BasicText(
                    description,
                    style = descriptionStyle.merge(TextStyle(color = descriptionColor, textAlign = TextAlign.Center)),
                    modifier = Modifier.padding(horizontal = descriptionPadding),
                )
            }
            when {
                action != null -> action()
                actionLabel != null && onAction != null -> {
                    val shape = RoundedCornerShape(percent = 50)
                    Box(
                        Modifier
                            .padding(top = 4.dp)
                            .height(tokens.pillHeight + 8.dp)
                            .clip(shape)
                            .background(tokens.card, shape)
                            .border(tokens.hairline, tokens.border, shape)
                            .clickable(role = Role.Button, onClick = onAction)
                            .padding(horizontal = 12.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        BasicText(
                            actionLabel,
                            style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.Medium, color = tokens.foreground),
                        )
                    }
                }
            }
        }
    }
}
