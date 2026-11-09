package at.exponential.ui.compose

import androidx.compose.runtime.Composable
import at.exponential.ui.model.embed

/**
 * #EDE-65 (fixtures/embed.json): an `Embed` leaf, its nested surface
 * painted in place the way the host's own is ([ExponentialSurface]: the
 * card's root at its core's frames, its layers, its keys), as wide as the
 * Embed's frame and as tall as the card. The nested model ([embed]) is the
 * one the measurer laid out: its own data and ids, the host's theme, mode,
 * settings, icons and policies; a bad `surface` shows the error box; a
 * changed `surface` value is a fresh mount.
 */
@Composable
internal fun EmbedLeaf(cx: LeafContext) {
    // A lookup (the measurer mounted it during the host's pass); it mounts
    // here only under the fixed measure (no measurer runs).
    val nested = cx.model.embed(cx.index) ?: return
    LeafFrame {
        ExponentialSurface(nested)
    }
}
