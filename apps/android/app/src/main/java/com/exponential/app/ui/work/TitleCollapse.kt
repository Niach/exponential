package com.exponential.app.ui.work

import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import com.exponential.app.domain.DetailChrome

/**
 * EXP-1162: the Work screen's title-collapse input. The header reports its
 * band's bottom edge, the Issue face its title row's — both in root px — and
 * only the resulting BOOLEAN is state: a scroll pixel writes nothing until the
 * title row crosses the band (`DetailChrome.isTitleCollapsed`). Every face
 * stays composed in the pager, but only the Issue face has a title row, and
 * the host reads [issueTitleCollapsed] only while the Issue face is shown.
 */
@Stable
class TitleCollapseState {
    var issueTitleCollapsed by mutableStateOf(false)
        private set

    private var headerBottom: Float? = null
    private var titleBottom: Float? = null

    fun reportHeaderBottom(bottom: Float) {
        if (bottom == headerBottom) return
        headerBottom = bottom
        recompute()
    }

    fun reportTitleBottom(bottom: Float?) {
        if (bottom == titleBottom) return
        titleBottom = bottom
        recompute()
    }

    private fun recompute() {
        val header = headerBottom ?: return
        val next = DetailChrome.isTitleCollapsed(
            hasTitleRow = true,
            titleBottom = titleBottom?.toDouble(),
            headerBottom = header.toDouble(),
        )
        if (next != issueTitleCollapsed) issueTitleCollapsed = next
    }
}

/** The Work screen's collapse state; null outside it (no reporting). */
val LocalTitleCollapse = compositionLocalOf<TitleCollapseState?> { null }

/** Reports this node — the Issue face's title row — as the collapse input. */
fun Modifier.reportsTitleRow(state: TitleCollapseState?): Modifier =
    if (state == null) {
        this
    } else {
        // Unclipped: `boundsInRoot` would pin a scrolled-away row to the
        // viewport's edge instead of letting it travel past the band.
        onGloballyPositioned { state.reportTitleBottom(it.positionInRoot().y + it.size.height) }
    }
