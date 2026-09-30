package com.exponential.app.domain

/**
 * EXP-1031: THE toast, identical on every client. The pure half: kinds, the
 * item model and the sonner-derived stack geometry, locked to
 * `packages/domain-contract/fixtures/toast-stack.json` by ToastStackTest. The
 * Compose half (card, host, [com.exponential.app.ui.components.Toaster]) lives
 * in `ui/components/Toast.kt` and only draws what [ToastStack.geometry] says.
 */
enum class ToastKind(val wire: String) {
    Success("success"),
    Error("error"),
    Info("info"),
    Warning("warning"),
}

/** The one optional button a toast carries; firing it also dismisses. */
data class ToastAction(val label: String, val onClick: () -> Unit)

/** One toast: a one-sentence [title], an optional [description] and [action]. */
data class ToastItem(
    val id: String,
    val kind: ToastKind,
    val title: String,
    val description: String? = null,
    val action: ToastAction? = null,
)

object ToastStack {
    /** The fixture's `constants`, sonner 2.0.7's numbers (points = dp). */
    object Constants {
        const val WIDTH = 356.0
        const val GAP = 14.0
        const val PEEK = 14.0
        const val SCALE_STEP = 0.05
        const val VISIBLE = 3
        const val DURATION_MS = 4000L
        const val SWIPE_THRESHOLD = 45.0
        const val VIEWPORT_OFFSET = 24.0
        const val MOBILE_VIEWPORT_OFFSET = 16.0
        val KINDS: List<String> = ToastKind.entries.map { it.wire }
    }

    /** One item's placement: [offset] = its top inside the stack box, [scale] its width factor. */
    data class ItemGeometry(val offset: Double, val scale: Double, val visible: Boolean)

    /** The stack box [height] plus every item's geometry, oldest first. */
    data class Layout(val height: Double, val items: List<ItemGeometry>)

    /**
     * [heights] = the measured toast heights, OLDEST first (the newest is the
     * last and draws in front). Offsets always measure from the box's top,
     * [anchoredBottom] only decides which edge the front toast hugs.
     */
    fun geometry(heights: List<Double>, expanded: Boolean, anchoredBottom: Boolean): Layout {
        val count = heights.size
        if (count == 0) return Layout(0.0, emptyList())
        val c = Constants
        fun rank(index: Int) = count - 1 - index
        val height = if (expanded) {
            heights.sum() + c.GAP * (count - 1)
        } else {
            val front = heights.last() + c.PEEK * (count - 1)
            heights.indices.maxOf { heights[it] + c.PEEK * rank(it) }.coerceAtLeast(front)
        }
        val items = heights.indices.map { index ->
            val r = rank(index)
            val h = heights[index]
            if (expanded) {
                val newer = heights.subList(index + 1, count).sum()
                val top = newer + r * c.GAP
                ItemGeometry(
                    offset = if (anchoredBottom) height - top - h else top,
                    scale = 1.0,
                    visible = true,
                )
            } else {
                ItemGeometry(
                    offset = if (anchoredBottom) height - h - r * c.PEEK else r * c.PEEK,
                    scale = 1.0 - c.SCALE_STEP * minOf(r, c.VISIBLE - 1),
                    visible = r < c.VISIBLE,
                )
            }
        }
        return Layout(height, items)
    }
}
