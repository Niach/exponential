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

        /** `placement.pointer`: web from [TOUCH_MAX_WIDTH] up and the IDE. */
        const val PLACEMENT_POINTER = "bottom-right"

        /** `placement.touch`: phones (Android, iOS, mobile web) stack from the TOP. */
        const val PLACEMENT_TOUCH = "top-center"

        /** Below this width (px/dp) a surface is `touch`. */
        const val TOUCH_MAX_WIDTH = 600.0
        val KINDS: List<String> = ToastKind.entries.map { it.wire }
    }

    /** One item's placement: [offset] = its top inside the stack box, [scale] its width factor. */
    data class ItemGeometry(val offset: Double, val scale: Double, val visible: Boolean)

    /** The stack box [height] plus every item's geometry, oldest first. */
    data class Layout(val height: Double, val items: List<ItemGeometry>)

    /** The toast's numbers as [StackConstants] (the Dock pile has its own, `Dock.STACK`). */
    val TOAST: StackConstants = StackConstants(
        peek = Constants.PEEK,
        scaleStep = Constants.SCALE_STEP,
        visible = Constants.VISIBLE,
        gap = Constants.GAP,
    )

    /**
     * [heights] = the measured toast heights, OLDEST first (the newest is the
     * last and draws in front). Offsets always measure from the box's top,
     * [anchoredBottom] only decides which edge the front toast hugs.
     */
    fun geometry(heights: List<Double>, expanded: Boolean, anchoredBottom: Boolean): Layout =
        stackLayout(heights, expanded, anchoredBottom, TOAST)

    /**
     * N-12: THE stack geometry ×4 (TS `stackLayout`, src/stack.ts), the toast's
     * shared with the Dock pile (dock.json `stack`): the newest item in front,
     * the older ones peeking out behind it by `peek` and shrinking by
     * `scaleStep` per rank, `visible` of them at most; expanded = every item
     * at full size, `gap` apart.
     */
    fun stackLayout(
        heights: List<Double>,
        expanded: Boolean,
        anchoredBottom: Boolean,
        constants: StackConstants,
    ): Layout {
        val count = heights.size
        fun rank(index: Int) = count - 1 - index
        // Distance of each item's anchor-side edge from the box's anchor edge.
        val lead = heights.indices.map { i ->
            if (expanded) {
                heights.subList(i + 1, count).sumOf { it + constants.gap }
            } else {
                constants.peek * rank(i)
            }
        }
        val height = heights.indices.maxOfOrNull { lead[it] + heights[it] } ?: 0.0
        val items = heights.indices.map { i ->
            val r = rank(i)
            val offset = if (anchoredBottom) height - lead[i] - heights[i] else lead[i]
            if (expanded) {
                ItemGeometry(round(offset), 1.0, true)
            } else {
                ItemGeometry(
                    offset = round(offset),
                    scale = round(1.0 - constants.scaleStep * minOf(r, constants.visible - 1)),
                    visible = r < constants.visible,
                )
            }
        }
        return Layout(round(height), items)
    }

    private fun round(value: Double): Double = Math.round(value * 1e4) / 1e4
}

/** N-12: one stack's numbers (TS `StackConstants`): the toast's and the Dock pile's. */
data class StackConstants(
    /** How far an older item peeks out behind the one in front of it. */
    val peek: Double,
    /** How much smaller each rank behind the front draws. */
    val scaleStep: Double,
    /** How many items draw at all while collapsed. */
    val visible: Int,
    /** The space between items while expanded. */
    val gap: Double,
)
