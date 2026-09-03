package com.exponential.app.ui.markdown

import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.TextToolbar
import androidx.compose.ui.platform.TextToolbarStatus

/**
 * EXP-727 — the selection toolbar of a table CELL: the system's Cut / Copy /
 * Paste / Select all strip plus one "Delete table" item, the ONE table
 * manipulation mobile ships. A long-press inside a cell already opens this
 * toolbar, so the action rides it instead of a second long-press gesture
 * competing with the field's own (which would cost cells their text
 * selection). iOS puts the same item on the cell's edit menu.
 *
 * Compose's own `AndroidTextToolbar` is final and takes no extra items, so
 * this is the same floating [ActionMode] built by hand: one callback whose
 * fields are refreshed on every [showMenu] and re-read by `invalidate()`.
 */
internal class TableCellTextToolbar(
    private val view: View,
    private val onDeleteTable: () -> Unit,
) : TextToolbar {
    private var actionMode: ActionMode? = null
    private val callback = Callback()

    override var status: TextToolbarStatus = TextToolbarStatus.Hidden
        private set

    override fun showMenu(
        rect: Rect,
        onCopyRequested: (() -> Unit)?,
        onPasteRequested: (() -> Unit)?,
        onCutRequested: (() -> Unit)?,
        onSelectAllRequested: (() -> Unit)?,
    ) {
        callback.rect = rect
        callback.onCopy = onCopyRequested
        callback.onPaste = onPasteRequested
        callback.onCut = onCutRequested
        callback.onSelectAll = onSelectAllRequested
        val mode = actionMode
        if (mode == null) {
            status = TextToolbarStatus.Shown
            actionMode = view.startActionMode(callback, ActionMode.TYPE_FLOATING)
        } else {
            mode.invalidate()
        }
    }

    override fun hide() {
        status = TextToolbarStatus.Hidden
        actionMode?.finish()
        actionMode = null
    }

    private inner class Callback : ActionMode.Callback2() {
        var rect: Rect = Rect.Zero
        var onCopy: (() -> Unit)? = null
        var onPaste: (() -> Unit)? = null
        var onCut: (() -> Unit)? = null
        var onSelectAll: (() -> Unit)? = null

        override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
            addItems(menu)
            return true
        }

        override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean {
            menu.clear()
            addItems(menu)
            return true
        }

        // Same ids, order and labels as Compose's TextActionModeCallback, then
        // ours; the platform strings keep the four system items localized.
        private fun addItems(menu: Menu) {
            if (onCopy != null) menu.addAlways(ID_COPY, 0, android.R.string.copy)
            if (onPaste != null) menu.addAlways(ID_PASTE, 1, android.R.string.paste)
            if (onCut != null) menu.addAlways(ID_CUT, 2, android.R.string.cut)
            if (onSelectAll != null) menu.addAlways(ID_SELECT_ALL, 3, android.R.string.selectAll)
            menu.add(0, ID_DELETE_TABLE, 4, "Delete table").setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        }

        private fun Menu.addAlways(id: Int, order: Int, title: Int) {
            add(0, id, order, title).setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        }

        override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
            when (item.itemId) {
                ID_COPY -> onCopy?.invoke()
                ID_PASTE -> onPaste?.invoke()
                ID_CUT -> onCut?.invoke()
                ID_SELECT_ALL -> onSelectAll?.invoke()
                ID_DELETE_TABLE -> onDeleteTable()
                else -> return false
            }
            mode.finish()
            return true
        }

        override fun onDestroyActionMode(mode: ActionMode) {
            actionMode = null
            status = TextToolbarStatus.Hidden
        }

        override fun onGetContentRect(mode: ActionMode, view: View, outRect: android.graphics.Rect) {
            outRect.set(rect.left.toInt(), rect.top.toInt(), rect.right.toInt(), rect.bottom.toInt())
        }
    }

    private companion object {
        const val ID_COPY = 0
        const val ID_PASTE = 1
        const val ID_CUT = 2
        const val ID_SELECT_ALL = 3
        const val ID_DELETE_TABLE = 4
    }
}

/** The toolbar every cell of ONE table shares; [onDeleteTable] always reads the latest lambda. */
@Composable
internal fun rememberTableCellTextToolbar(onDeleteTable: () -> Unit): TextToolbar {
    val view = LocalView.current
    val current by rememberUpdatedState(onDeleteTable)
    return remember(view) { TableCellTextToolbar(view) { current() } }
}
