package com.exponential.app.ui.apps

import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.ToastAction
import com.exponential.app.ui.components.Toaster

// VAPP-215 (R4, app-records.json `history`; web `lib/apps/record-undo.ts`):
// the Undo toast a UI records write raises (the Data tab's add / save /
// delete, a package action's batch): the existing success toast + its action
// for `undoSeconds` s. [onUndo] runs `apps.records.undo`; its failure is the
// failed-action toast `Couldn't undo the change`.

/** A UI write to toast Undo for: [write] = `create` | `update` | `delete` | `batch`. */
data class RecordUndo(val write: String, val eventIds: List<String>, val via: String)

/** Toast [write] with Undo; no events (a batch that changed nothing) = no toast. */
fun Toaster.recordUndo(write: String, eventIds: List<String>, onUndo: () -> Unit) {
    if (eventIds.isEmpty()) return
    success(
        AppRecords.undoTitle(write),
        action = ToastAction(AppRecords.HistoryCopy.UNDO, onUndo),
        durationMs = AppRecords.History.UNDO_SECONDS * 1000L,
    )
}
