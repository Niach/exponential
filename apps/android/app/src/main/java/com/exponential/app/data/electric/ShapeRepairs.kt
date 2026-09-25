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

/**
 * Nothing shipped right now. The last entry, EXP-985's `exp985_issue_drafts`
 * refetch (builds before it REPLACEd a draft row with a partial `update`),
 * was retired once the `CLIENT_MIN_VERSION_ANDROID` floor passed the fixed
 * build: every device this build can still talk to has either run the repair
 * or never held the damage. A repair is added here as
 * `ShapeRepair(id = "<issue>_<shape>", shapes = listOf(...))` and its marker
 * outlives the entry, so re-adding a retired id is a no-op on healed devices.
 */
internal val SHAPE_REPAIRS = emptyList<ShapeRepair>()

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
