package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-893: the ONE floating bottom bar of the phone Work screen —
// `[left circle] [centre capsule] [right circle]` on the 52dp glass rung the
// issue bar and the tab bar already draw (near-opaque pill fill + hairline).
// Every face fills the three slots differently (Issue: properties · comment ·
// switcher; Run: usage ring · steer · switcher; Changes: GitHub · merge ·
// switcher), so the chrome is shared and only the slots move. A missing slot
// simply leaves its space empty, so the capsule never jumps between faces.
//
// EXP-916: the CHANGES bars (the Reviews page and the Work screen's Changes
// face) are a [FloatingBarCluster] instead — nothing stretches, the circles
// and the white [BarSolidPill] hug their content 12dp apart, centred. The
// web (`MobileWorkBar cluster` + `MergeCapsule`) and iOS
// (`FloatingBarCluster` + `FloatingBarSolidPill`) mirror both.

/** The bar's rung — the height of its capsule and the diameter of its circles. */
val FloatingBarRung: Dp = 52.dp

/**
 * The bar itself. [left] and [right] are optional circles (or anything else
 * 52dp tall); [centre] is the capsule that takes the remaining width. Padded
 * to the screen's 20dp gutter like the issue bar it replaces.
 */
@Composable
fun FloatingBottomBar(
    modifier: Modifier = Modifier,
    left: (@Composable () -> Unit)? = null,
    right: (@Composable () -> Unit)? = null,
    centre: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        left?.invoke()
        centre()
        right?.invoke()
    }
}

/**
 * EXP-916: the centred cluster — `[left] [centre] [right]` hugging their
 * content, 12dp apart. Every slot is optional and a missing one leaves no
 * gap. Same screen inset and vertical padding as [FloatingBottomBar].
 */
@Composable
fun FloatingBarCluster(
    modifier: Modifier = Modifier,
    left: (@Composable () -> Unit)? = null,
    centre: (@Composable () -> Unit)? = null,
    right: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally),
    ) {
        left?.invoke()
        centre?.invoke()
        right?.invoke()
    }
}

/**
 * EXP-706/EXP-916: the bar's labelled PRIMARY action — a SOLID white pill
 * with dark content, hugging its label (28dp padding, a 20dp glyph), the one
 * solid thing on a Changes bar. No hairline: a white fill needs no edge
 * against the dim circles beside it. A [loading] pill spins in place of its
 * glyph and drops the tap.
 */
@Composable
fun BarSolidPill(
    label: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
) {
    Row(
        modifier = modifier
            .height(FloatingBarRung)
            .clip(RoundedCornerShape(percent = 50))
            .background(Color.White)
            .clickable(enabled = enabled && !loading, role = Role.Button, onClick = onClick)
            .padding(horizontal = 28.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (loading) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                strokeWidth = 2.dp,
                // On white, the default primary tint disappears.
                color = Color.Black,
            )
        } else {
            Icon(
                icon,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = Color.Black,
            )
        }
        Text(
            label,
            style = MaterialTheme.typography.titleSmall,
            color = Color.Black,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * One 52dp glass circle of the bar. Public since EXP-893 (it lived in the
 * issue bar): the Work screen's switcher, usage ring and GitHub circles are
 * all this. [enabled] drops the tap but keeps the disc — a dimmed glyph is the
 * caller's job, so "Start coding with no desktop" can still explain itself.
 */
@Composable
fun BarCircle(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = modifier
            .size(FloatingBarRung)
            .clip(CircleShape)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, CircleShape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

/**
 * The bar's centre capsule: an optional leading glyph and a placeholder
 * label (tertiary white) that opens a composer. A verb never rides it —
 * Merge is the [BarSolidPill] in a [FloatingBarCluster]. A [loading]
 * capsule spins in place of its glyph and drops the tap.
 */
@Composable
fun RowScope.BarCapsule(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    loading: Boolean = false,
) {
    val capsule = RoundedCornerShape(percent = 50)
    val tint = Color.White.copy(alpha = TextEmphasis.Tertiary)
    Row(
        modifier = modifier
            .weight(1f)
            .height(FloatingBarRung)
            .clip(capsule)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, capsule)
            .clickable(enabled = enabled && !loading, role = Role.Button, onClick = onClick)
            .padding(horizontal = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when {
            loading -> CircularProgressIndicator(
                modifier = Modifier.size(16.dp),
                strokeWidth = 2.dp,
                color = Color.White,
            )
            icon != null -> Icon(
                icon,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = tint,
            )
        }
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = tint,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
