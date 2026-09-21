package com.exponential.app.data.electric

import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.ElectricOffsetEntity

/**
 * One-time, per-account shape repairs: a shape whose LOCAL rows an older build
 * corrupted is marked for the atomic refetch ([ElectricOffsetEntity.needsRefetch],
 * EXP-264 — the stale rows stay visible until the fresh snapshot replaces them
 * in one transaction), exactly once per account database.
 *
 * The "done" marker is a sentinel row in `electric_offsets` itself: it lives
 * and dies with the account database it describes (no Room version bump, whose
 * destructive fallback would re-sync all 24 shapes), and no shape is ever
 * named with the `_repair:` prefix.
 */
internal data class ShapeRepair(val id: String, val shapes: List<String>)

internal val SHAPE_REPAIRS = listOf(
    // EXP-985: builds before the fix decoded a wire `update` (changed columns
    // + PK only) into a full entity and REPLACEd the row. IssueDraftEntity is
    // the ONE entity whose every non-PK column has a default — every other
    // entity requires a non-PK column no update carries, so its decode failed
    // into the tolerant partial path — so only drafts lost columns (board_id,
    // and with it the Drafts tab). The fix stops new damage; this refetch
    // heals the rows already blanked in Room.
    ShapeRepair(id = "exp985_issue_drafts", shapes = listOf("issue_drafts")),
)

internal fun repairMarker(id: String, shape: String) = "_repair:$id:$shape"

/**
 * Runs at the head of [shape]'s OWN sync job, before its first poll, so
 * nothing races the marker and the very next poll is the refetch. The marker
 * is per (repair, shape): each shape loop settles only its own.
 */
internal suspend fun applyShapeRepairs(
    offsetDao: ElectricOffsetDao,
    shape: String,
    repairs: List<ShapeRepair> = SHAPE_REPAIRS,
) {
    for (repair in repairs) {
        if (shape !in repair.shapes) continue
        val marker = repairMarker(repair.id, shape)
        if (offsetDao.get(marker) != null) continue
        // No cursor = nothing synced yet: the first snapshot is the repair.
        offsetDao.get(shape)?.let { saved ->
            offsetDao.upsert(
                ElectricOffsetEntity(
                    shape = shape,
                    handle = saved.handle,
                    offset = INITIAL_OFFSET,
                    isLive = false,
                    needsRefetch = true,
                ),
            )
        }
        offsetDao.upsert(ElectricOffsetEntity(shape = marker, handle = "", offset = ""))
    }
}
