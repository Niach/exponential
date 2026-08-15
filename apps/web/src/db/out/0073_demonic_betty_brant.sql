-- EXP-500: archivable boards. Adds boards.archived_at — a NON-PURGING sibling
-- of the deleted_at trash — plus the trigger-maintained board_archived_at
-- mirror on issues and the 7 issue-child synced tables, so the board-scoped
-- shape where clauses can be `… AND board_archived_at IS NULL`, byte-stable
-- across archive/unarchive exactly like the REV2-5 trash mirror (migration
-- 0041). Archiving a board therefore removes it AND all of its history from
-- every client's sync scope and every server read surface until an owner
-- unarchives it, without a single client-side filter — the previous archiving
-- attempt synced the column and filtered by hand on all four clients, leaked,
-- and was deleted wholesale (REV2-103).
--
-- No backfill: no board is archived yet, so all-NULL is the correct initial
-- state. Hand-edited from the generated ALTERs to inline the trigger changes
-- (mirroring db/out/custom/0001_triggers.sql, which boot re-applies
-- idempotently) so there is no broken window between `migrate` and the next
-- server boot.
ALTER TABLE "attachments" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "boards" ADD COLUMN "archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "notifications" ADD COLUMN "board_archived_at" timestamp with time zone;--> statement-breakpoint
CREATE INDEX "idx_boards_archived" ON "boards" USING btree ("archived_at") WHERE archived_at IS NOT NULL;--> statement-breakpoint

-- The updated_at guards gain the new mirror: the archive fan-out below only
-- flips board_archived_at, and bumping updated_at there would stamp a whole
-- board's history as freshly edited on archive AND on unarchive.
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_labels FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at
    AND NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at
    AND NEW.board_id IS NOT DISTINCT FROM OLD.board_id)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint

-- The comment bump treats an archive fan-out as bookkeeping, not discussion.
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
  IF (TG_OP = 'UPDATE')
    AND (NEW.board_deleted_at IS DISTINCT FROM OLD.board_deleted_at
      OR NEW.board_archived_at IS DISTINCT FROM OLD.board_archived_at
      OR NEW.board_id IS DISTINCT FROM OLD.board_id) THEN
    RETURN NEW;
  END IF;
  IF (TG_OP = 'DELETE') THEN
    target_issue := OLD.issue_id;
  ELSE
    target_issue := NEW.issue_id;
  END IF;
  UPDATE issues SET updated_at = now() WHERE id = target_issue;
  IF (TG_OP = 'DELETE') THEN
    RETURN OLD;
  ELSE
    RETURN NEW;
  END IF;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

-- Both populate triggers now derive the archive mirror alongside the trash one.
CREATE OR REPLACE FUNCTION populate_issue_child_board_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT i.board_id, b.deleted_at, b.archived_at
      INTO NEW.board_id, NEW.board_deleted_at, NEW.board_archived_at
    FROM issues i JOIN boards b ON b.id = i.board_id
    WHERE i.id = NEW.issue_id
    FOR KEY SHARE OF i;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE FUNCTION populate_issue_board_context()
RETURNS TRIGGER AS $$
BEGIN
  SELECT b.team_id, b.deleted_at, b.archived_at
    INTO NEW.team_id, NEW.board_deleted_at, NEW.board_archived_at
  FROM boards b
  WHERE b.id = NEW.board_id
  FOR SHARE OF b;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

-- The archive fan-out itself: one indexed UPDATE per child table, emitting
-- incremental Electric move-out/move-in deltas instead of rotating the shape
-- identities. Kept separate from propagate_board_deleted_at so each fan-out
-- only runs when its own column changed.
CREATE OR REPLACE FUNCTION propagate_board_archived_at()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE issues SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE comments SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE attachments SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_labels SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_subscribers SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE issue_events SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE coding_sessions SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  UPDATE notifications SET board_archived_at = NEW.archived_at
    WHERE board_id = NEW.id AND board_archived_at IS DISTINCT FROM NEW.archived_at;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER propagate_board_archived_at
  AFTER UPDATE ON boards
  FOR EACH ROW
  WHEN (OLD.archived_at IS DISTINCT FROM NEW.archived_at)
  EXECUTE FUNCTION propagate_board_archived_at();
