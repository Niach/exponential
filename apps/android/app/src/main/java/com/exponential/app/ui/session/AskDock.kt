package com.exponential.app.ui.session

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.AskDock as AskDockModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// THE ask dock ×Android (fixtures/ask-dock.json ×4; web packages/ui
// ask-dock.tsx): the question/plan cards waiting on the viewer take the
// composer's slot, in the session view and the thread alike. `single` /
// `focused` = THE big card (the host draws it: QuestionCard / the stepper /
// ThreadAskCard, reading [LocalAskDockCard] for the body cap and the back
// arrow); `stack` = compact AskCards 6dp apart, a tap grows one into the big
// card. The swap animates the dock's height over motion `standard` and fades
// the new content in (decelerate); reduce motion = opacity only.

/** What a big card drawn INSIDE the dock reads: its body's height cap (the
 *  fixture's `bodyMaxHeightFraction` of the screen; the body scrolls inside,
 *  never folds behind Show more) and, when focused out of a stack, the back
 *  arrow leading its header. Null = a transcript card. */
internal data class AskDockCardChrome(val bodyMaxHeight: Dp, val onBack: (() -> Unit)?)

internal val LocalAskDockCard = compositionLocalOf<AskDockCardChrome?> { null }

/** The dock over [view]; [card] draws the big card for an id (the host's
 *  card, under [LocalAskDockCard]). Draws nothing in mode `none`. */
@Composable
internal fun AskDock(
    view: AskDockModel.View,
    onFocus: (String) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    card: @Composable (id: String) -> Unit,
) {
    if (view.mode == AskDockModel.Mode.None) return
    val reduce = LocalReduceMotion.current
    val bodyMax = (LocalConfiguration.current.screenHeightDp * AskDockModel.BODY_MAX_HEIGHT_FRACTION).dp
    AnimatedContent(
        targetState = view,
        // A swap (stack ⇄ a card) re-mounts and fades in; the same content
        // updating in place (a row answered off the stack) does not.
        contentKey = { it.cardId?.let { id -> "card:$id" } ?: "stack" },
        transitionSpec = {
            (
                fadeIn(tween(DesignTokens.Motion.Duration.Standard, easing = DesignTokens.Motion.Ease.Decelerate)) togetherWith
                    fadeOut(tween(DesignTokens.Motion.Duration.Fast, easing = DesignTokens.Motion.Ease.Accelerate))
                ).using(SizeTransform(clip = true) { _, _ -> if (reduce) snap() else Motion.standard(false) })
        },
        modifier = modifier.fillMaxWidth().testTag(AskDockModel.TestIds.DOCK),
        label = "ask-dock",
    ) { shown ->
        // In-place growth (an option's inline field opening) animates too.
        val grow = if (reduce) Modifier else Modifier.animateContentSize(Motion.standard(false))
        val cardId = shown.cardId
        if (cardId != null) {
            Box(Modifier.fillMaxWidth().then(grow).testTag(AskDockModel.TestIds.CARD)) {
                androidx.compose.runtime.CompositionLocalProvider(
                    LocalAskDockCard provides AskDockCardChrome(bodyMax, if (shown.back) onBack else null),
                ) { card(cardId) }
            }
        } else {
            Column(
                modifier = Modifier.fillMaxWidth().then(grow),
                verticalArrangement = Arrangement.spacedBy(AskDockModel.STACK_GAP.dp),
            ) {
                shown.rows.forEach { row -> AskDockStackCard(row, onOpen = { onFocus(row.id) }) }
            }
        }
    }
}

/** A compact card in the stack: the AskCard chrome (icon + tinted label +
 *  the session caption) over the one-line title and the open chevron. */
@Composable
internal fun AskDockStackCard(row: AskDockModel.Row, onOpen: () -> Unit) {
    AskCard(
        plan = row.kind == AskDockModel.Kind.Plan,
        heading = row.label,
        meta = row.caption,
        modifier = Modifier
            .clip(RoundedCornerShape(GlassTokens.CardRadius))
            .clickable(role = Role.Button, onClick = onOpen)
            .semantics(mergeDescendants = true) { contentDescription = "${row.label}: ${row.title}" }
            .testTag(AskDockModel.TestIds.ROW),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                row.title,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.bodyMedium.copy(fontSize = 14.sp),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.9f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Icon(
                ExpIcons.uiChevronRight,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** The focused card's back arrow: a small ghost icon button (web `ghost
 *  icon-xs`, 24dp) LEADING the card's header row, pulled 4dp up and left so
 *  it never grows the header. */
@Composable
internal fun AskDockBackButton(onBack: () -> Unit) {
    Box(
        modifier = Modifier
            .size(24.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClickLabel = AskDockModel.Copy.BACK, onClick = onBack)
            .semantics { contentDescription = AskDockModel.Copy.BACK }
            .testTag(AskDockModel.TestIds.BACK),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.uiBack,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}
