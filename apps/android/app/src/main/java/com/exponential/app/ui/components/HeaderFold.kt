package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import com.exponential.app.domain.HeaderActions

/**
 * #EXP-2183 (header-actions.json COLLAPSE): a narrow header folds its Merge
 * pill (`primary`), then its waiting chip (`status`), to the glyph so the
 * title keeps [HeaderActions.TITLE_MIN_SHARE] of the bar. The header
 * ([com.exponential.app.ui.chat.ChatHeader]) owns one, measures the bar and
 * the title; each chip reports its widths with [headerFoldChip] and reads
 * [isFolded]. No header (null) = nothing folds.
 */
@Stable
class HeaderFold {
    var folded by mutableStateOf<Set<String>>(emptySet())
        private set
    var bar by mutableFloatStateOf(0f)
    var titleSpace by mutableFloatStateOf(0f)
    var titleIntrinsic by mutableFloatStateOf(0f)
    private val placed = mutableStateMapOf<String, Float>()
    private val full = mutableStateMapOf<String, Pair<Any?, Float>>()
    private val compact = mutableStateMapOf<String, Float>()

    fun isFolded(action: String): Boolean = action in folded

    internal fun report(action: String, key: Any?, width: Float, compactEstimate: Float) {
        placed[action] = width
        if (action in folded) {
            compact[action] = width
            // The content changed while folded: unknown full width, measure it once.
            if (full[action]?.first != key) full.remove(action)
        } else {
            full[action] = key to width
            if (action !in compact) compact[action] = compactEstimate
        }
    }

    internal fun forget(action: String) {
        placed.remove(action)
        full.remove(action)
        compact.remove(action)
    }

    /** The fold the latest measures ask for (reads state: run it in a
     *  snapshotFlow); null = nothing measured yet. */
    fun decide(): Set<String>? {
        if (bar <= 0f) return null
        val fixed = bar - titleSpace - placed.values.sum()
        val chips = full.mapNotNull { (action, entry) ->
            HeaderActions.CollapseChip(action, entry.second, compact[action] ?: return@mapNotNull null)
        }
        return HeaderActions.collapse(bar, titleIntrinsic, fixed, chips).toSet()
    }

    /** Follows the measures for the header's life. */
    suspend fun follow() {
        snapshotFlow { decide() }.collect { next -> if (next != null && next != folded) folded = next }
    }
}

val LocalHeaderFold = staticCompositionLocalOf<HeaderFold?> { null }

/** A foldable chip's report: its placed width (keyed by its content, [key])
 *  and the glyph-only width it folds to ([compact], until measured). */
@Composable
fun Modifier.headerFoldChip(action: String, key: Any?, compact: Dp): Modifier {
    val fold = LocalHeaderFold.current ?: return this
    val compactPx = with(LocalDensity.current) { compact.toPx() }
    DisposableEffect(fold, action) { onDispose { fold.forget(action) } }
    return onSizeChanged { fold.report(action, key, it.width.toFloat(), compactPx) }
}

/** Whether the enclosing header folds [action] to its glyph. */
@Composable
fun headerFolded(action: String): Boolean = LocalHeaderFold.current?.isFolded(action) == true
