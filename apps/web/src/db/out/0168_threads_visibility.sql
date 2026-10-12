ALTER TABLE "threads" ADD COLUMN "has_visible_items" boolean DEFAULT true NOT NULL;--> statement-breakpoint
-- EXP-1284 review (T3) backfill. The custom triggers (populate_thread_item_
-- board_id for run items, refresh_thread_visibility) are applied at boot,
-- AFTER this runs, so the state they keep is written explicitly here. The
-- transaction-local GUC keeps update_updated_at from stamping every thread
-- and item as freshly active (lists sort threads by updated_at).
SELECT set_config('exponential.preserve_timestamps', 'on', true);
--> statement-breakpoint
-- Run items mirror their SESSION's board context (0166 left it NULL).
UPDATE thread_items ti
SET board_id = cs.board_id,
  board_deleted_at = cs.board_deleted_at,
  board_archived_at = cs.board_archived_at
FROM coding_sessions cs
WHERE ti.session_id = cs.id
  AND (ti.board_id IS DISTINCT FROM cs.board_id
    OR ti.board_deleted_at IS DISTINCT FROM cs.board_deleted_at
    OR ti.board_archived_at IS DISTINCT FROM cs.board_archived_at);
--> statement-breakpoint
-- A thread whose items are all gone and whose Guide is empty is pruned.
DELETE FROM threads t
WHERE NOT EXISTS (SELECT 1 FROM thread_items ti WHERE ti.thread_id = t.id)
  AND (t.results IS NULL OR jsonb_typeof(t.results) <> 'array' OR jsonb_array_length(t.results) = 0);
--> statement-breakpoint
-- Hidden = no visible item left (every item on a trashed/archived board, or
-- none at all with a Guide still on it).
UPDATE threads t
SET has_visible_items = false
WHERE NOT EXISTS (
  SELECT 1 FROM thread_items ti
  WHERE ti.thread_id = t.id AND ti.board_deleted_at IS NULL AND ti.board_archived_at IS NULL
);
