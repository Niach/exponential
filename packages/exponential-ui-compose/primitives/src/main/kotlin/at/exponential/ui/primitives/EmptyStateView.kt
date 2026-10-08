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
 * The empty state every client draws, centred: an [icon] (on an
 * [iconDisc] circle of [iconDiscSize] unless the disc is null), a
 * [message], a [detail] line and one action: the [action] slot, or a
 * capsule button from [actionLabel] + [onAction]. Text styles and colours
 * default from the tokens.
 */
@Composable
fun EmptyStateView(
    message: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    icon: (@Composable () -> Unit)? = null,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
    action: (@Composable () -> Unit)? = null,
    iconDisc: Color? = LocalPrimitiveTokens.current.muted,
    iconDiscSize: Dp = 40.dp,
    iconColor: Color = LocalPrimitiveTokens.current.mutedForeground,
    messageStyle: TextStyle = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Medium),
    messageColor: Color = LocalPrimitiveTokens.current.foreground,
    detailStyle: TextStyle = TextStyle(fontSize = 13.sp),
    detailColor: Color = LocalPrimitiveTokens.current.mutedForeground,
    detailPadding: Dp = 0.dp,
    spacing: Dp = 8.dp,
    contentPadding: PaddingValues = PaddingValues(24.dp),
) {
    val tokens = LocalPrimitiveTokens.current
    Box(modifier = modifier.padding(contentPadding), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(spacing),
        ) {
            if (icon != null) {
                CompositionLocalProvider(LocalContentColor provides iconColor) {
                    if (iconDisc != null) {
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
                message,
                style = messageStyle.merge(TextStyle(color = messageColor, textAlign = TextAlign.Center)),
            )
            if (detail != null) {
                BasicText(
                    detail,
                    style = detailStyle.merge(TextStyle(color = detailColor, textAlign = TextAlign.Center)),
                    modifier = Modifier.padding(horizontal = detailPadding),
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

