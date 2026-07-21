-- REV2-5: stable (team-scoped) shape identities for the 8 board-scoped
-- shapes. Adds issues.team_id plus a trigger-maintained board_deleted_at
-- mirror column on issues and the 7 issue-child synced tables, so the shape
-- where clauses can be `team_id IN (…) AND board_deleted_at IS NULL` —
-- byte-stable across board create/trash/restore — instead of embedding the
-- caller's full board-id set (which rotated every board-scoped shape
-- identity for every team member on any board create/trash and forced full
-- cross-team resyncs). Hand-edited from the generated ALTERs: issues.team_id
-- is added nullable, backfilled from boards, then locked NOT NULL; the
-- trigger changes are inlined (mirroring db/out/custom/0001_triggers.sql,
-- which boot re-applies idempotently) so there is no broken window between
-- `migrate` and the next server boot.
ALTER TABLE "attachments" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "team_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "notifications" ADD COLUMN "board_deleted_at" timestamp with time zone;--> statement-breakpoint

-- Backfill issues from the parent board, then lock team_id down.
UPDATE "issues" i
SET "team_id" = b."team_id", "board_deleted_at" = b."deleted_at"
FROM "boards" b
WHERE i."board_id" = b."id";--> statement-breakpoint
ALTER TABLE "issues" ALTER COLUMN "team_id" SET NOT NULL;--> statement-breakpoint

-- Backfill the child mirrors for boards already in the trash (live boards'
-- children keep the NULL default).
UPDATE "comments" c SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE c."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "attachments" a SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE a."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "issue_labels" il SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE il."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "issue_subscribers" s SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE s."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "issue_events" e SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE e."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "coding_sessions" cs SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE cs."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint
UPDATE "notifications" n SET "board_deleted_at" = b."deleted_at"
FROM "boards" b WHERE n."board_id" = b."id" AND b."deleted_at" IS NOT NULL;--> statement-breakpoint

ALTER TABLE "issues" ADD CONSTRAINT "issues_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issues_team" ON "issues" USING btree ("team_id");--> statement-breakpoint

-- Trigger updates (inlined copy of the changed 0001_triggers.sql sections).
-- 1) updated_at bumps skip the trash/restore fan-out (which only flips
--    board_deleted_at) so restoring a board doesn't stamp its whole history
--    as freshly edited. The function body is unchanged but defined here too:
--    on a FRESH database the migrations run before boot ever applies
--    0001_triggers.sql, so the trigger recreations below would otherwise
--    reference a not-yet-existing function.
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issues FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON comments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON attachments FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON notifications FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_subscribers FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_events FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint
CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON coding_sessions FOR EACH ROW
  WHEN (NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at)
  EXECUTE FUNCTION update_updated_at();--> statement-breakpoint

-- 2) Comment bumps skip the fan-out too (it is bookkeeping, not discussion).
CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()
RETURNS TRIGGER AS $$
DECLARE
  target_issue uuid;
BEGIN
  IF (TG_OP = 'UPDATE')
    AND (NEW.board_deleted_at IS DISTINCT FROM OLD.board_deleted_at) THEN
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

-- 3) The child board_id populate also derives board_deleted_at, and fires on
--    UPDATE OF board_id so issues.move re-derives both columns.
CREATE OR REPLACE FUNCTION populate_issue_child_board_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT i.board_id, b.deleted_at INTO NEW.board_id, NEW.board_deleted_at
    FROM issues i JOIN boards b ON b.id = i.board_id
    WHERE i.id = NEW.issue_id
    FOR KEY SHARE OF i;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_comment_board_id
  BEFORE INSERT OR UPDATE OF board_id ON comments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_attachment_board_id
  BEFORE INSERT OR UPDATE OF board_id ON attachments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_issue_event_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_issue_subscriber_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_coding_session_board_id
  BEFORE INSERT OR UPDATE OF board_id ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_issue_label_board_id
  BEFORE INSERT OR UPDATE OF board_id ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint
CREATE OR REPLACE TRIGGER populate_notification_board_id
  BEFORE INSERT OR UPDATE OF board_id ON notifications
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

-- 4) issues derive team_id + board_deleted_at from the parent board.
CREATE OR REPLACE FUNCTION populate_issue_board_context()
RETURNS TRIGGER AS $$
BEGIN
  SELECT b.team_id, b.deleted_at INTO NEW.team_id, NEW.board_deleted_at
  FROM boards b
  WHERE b.id = NEW.board_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_board_context
  BEFORE INSERT OR UPDATE OF board_id ON issues
  FOR EACH ROW EXECUTE FUNCTION populate_issue_board_context();--> statement-breakpoint

-- 5) Board trash/restore fans deleted_at out to the child mirrors, turning
--    trash into incremental shape move-out/move-in deltas instead of a
--    shape-identity rotation.
CREATE OR REPLACE FUNCTION propagate_board_deleted_at()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE issues SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE comments SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE attachments SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_labels SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_subscribers SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE issue_events SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE coding_sessions SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  UPDATE notifications SET board_deleted_at = NEW.deleted_at
    WHERE board_id = NEW.id AND board_deleted_at IS DISTINCT FROM NEW.deleted_at;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER propagate_board_deleted_at
  AFTER UPDATE ON boards
  FOR EACH ROW
  WHEN (OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
  EXECUTE FUNCTION propagate_board_deleted_at();
