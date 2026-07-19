-- EXP-180 great rename: workspace → team, project → board. Hand-written
-- RENAMEs (data is kept); the drizzle snapshot for this migration was
-- generated from the renamed schema, so a follow-up `migrate:generate`
-- reports no changes. Order: drop the populate triggers whose bodies
-- reference old column names → enum renames → table renames → column
-- renames → constraint/index renames (catalog-driven, so historically
-- truncated names rename too) → event payload key rewrite → re-create the
-- triggers with the new vocabulary (mirrors db/out/custom/0001_triggers.sql,
-- which boot re-applies idempotently).

-- 1. Old populate/generate triggers + functions: their bodies read
--    workspace_id/project_id and would break every INSERT after the rename.
--    (update_updated_at and bump_issue_updated_at_from_comment reference no
--    renamed identifiers and survive as-is.)
DROP TRIGGER IF EXISTS generate_issue_number ON "issues";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_label_workspace_id ON "issue_labels";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_label_project_id ON "issue_labels";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_subscriber_workspace_id ON "issue_subscribers";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_subscriber_project_id ON "issue_subscribers";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_event_workspace_id ON "issue_events";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_issue_event_project_id ON "issue_events";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_coding_session_workspace_id ON "coding_sessions";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_coding_session_project_id ON "coding_sessions";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_comment_project_id ON "comments";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_attachment_project_id ON "attachments";--> statement-breakpoint
DROP TRIGGER IF EXISTS populate_notification_project_id ON "notifications";--> statement-breakpoint
DROP FUNCTION IF EXISTS generate_issue_number();--> statement-breakpoint
DROP FUNCTION IF EXISTS populate_issue_label_workspace_id();--> statement-breakpoint
DROP FUNCTION IF EXISTS populate_issue_child_workspace_id();--> statement-breakpoint
DROP FUNCTION IF EXISTS populate_issue_child_project_id();--> statement-breakpoint

-- 2. Enum renames
ALTER TYPE "public"."workspace_member_role" RENAME TO "team_member_role";--> statement-breakpoint
ALTER TYPE "public"."issue_event_type" RENAME VALUE 'project_moved' TO 'board_moved';--> statement-breakpoint

-- 3. Table renames
ALTER TABLE "workspaces" RENAME TO "teams";--> statement-breakpoint
ALTER TABLE "workspace_members" RENAME TO "team_members";--> statement-breakpoint
ALTER TABLE "workspace_invites" RENAME TO "team_invites";--> statement-breakpoint
ALTER TABLE "projects" RENAME TO "boards";--> statement-breakpoint

-- 4. Column renames
ALTER TABLE "team_members" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "team_invites" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "boards" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "creem_subscriptions" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "issues" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "issue_number_counters" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "labels" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "issue_labels" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "issue_labels" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "comments" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "comments" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "attachments" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "attachments" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "coding_sessions" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "coding_sessions" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "notifications" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "issue_subscribers" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "issue_subscribers" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "issue_events" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "issue_events" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "repositories" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "run_configs" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "run_configs" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "widget_configs" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "widget_configs" RENAME COLUMN "project_id" TO "board_id";--> statement-breakpoint
ALTER TABLE "support_threads" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "github_installation_links" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "github_installation_repo_grants" RENAME COLUMN "workspace_id" TO "team_id";--> statement-breakpoint
ALTER TABLE "mcp_grants" RENAME COLUMN "all_workspaces" TO "all_teams";--> statement-breakpoint
ALTER TABLE "mcp_grants" RENAME COLUMN "workspace_ids" TO "team_ids";--> statement-breakpoint
ALTER TABLE "mcp_grants" RENAME COLUMN "project_ids" TO "board_ids";--> statement-breakpoint

-- 5. Constraint + index renames to drizzle-canonical names. Catalog-driven so
--    names Postgres truncated at 63 chars rename correctly too; renaming a
--    constraint renames its backing index, the second loop catches the rest.
DO $$
DECLARE
  r record;
  newname text;
BEGIN
  FOR r IN
    SELECT c.conname AS name, c.conrelid::regclass::text AS tbl
    FROM pg_constraint c
    JOIN pg_class t ON t.oid = c.conrelid
    JOIN pg_namespace n ON n.oid = t.relnamespace
    WHERE n.nspname = 'public'
      AND (c.conname LIKE '%workspace%' OR c.conname LIKE '%project%')
  LOOP
    newname := replace(replace(r.name, 'workspace', 'team'), 'project', 'board');
    EXECUTE format('ALTER TABLE %s RENAME CONSTRAINT %I TO %I', r.tbl, r.name, newname);
  END LOOP;
  FOR r IN
    SELECT indexname AS name
    FROM pg_indexes
    WHERE schemaname = 'public'
      AND (indexname LIKE '%workspace%' OR indexname LIKE '%project%')
  LOOP
    newname := replace(replace(r.name, 'workspace', 'team'), 'project', 'board');
    EXECUTE format('ALTER INDEX %I RENAME TO %I', r.name, newname);
  END LOOP;
END $$;--> statement-breakpoint

-- 6. board_moved event payloads: clients read fromBoardId/toBoardId now.
UPDATE "issue_events"
SET payload = (payload - 'fromProjectId' - 'toProjectId')
  || jsonb_build_object(
    'fromBoardId', payload->'fromProjectId',
    'toBoardId', payload->'toProjectId'
  )
WHERE type = 'board_moved' AND payload ? 'fromProjectId';--> statement-breakpoint

-- 7. Re-create the dropped triggers with the new vocabulary. Mirrors
--    db/out/custom/0001_triggers.sql (idempotent — boot re-applies the same
--    file), inlined here so there is no broken window between `migrate` and
--    the next server boot.
CREATE OR REPLACE FUNCTION generate_issue_number()
RETURNS TRIGGER AS $$
DECLARE
  next_number integer;
  current_max integer;
  board_prefix text;
BEGIN
  SELECT COALESCE(MAX(number), 0) INTO current_max
  FROM issues
  WHERE board_id = NEW.board_id;

  INSERT INTO issue_number_counters AS c (board_id, counter)
  VALUES (NEW.board_id, current_max + 1)
  ON CONFLICT (board_id) DO UPDATE
    SET counter = GREATEST(c.counter, current_max) + 1
  RETURNING counter INTO next_number;

  SELECT prefix INTO board_prefix
  FROM boards
  WHERE id = NEW.board_id;

  NEW.number := next_number;
  NEW.identifier := board_prefix || '-' || next_number;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER generate_issue_number BEFORE INSERT ON issues FOR EACH ROW EXECUTE FUNCTION generate_issue_number();--> statement-breakpoint

CREATE OR REPLACE FUNCTION populate_issue_label_team_id()
RETURNS TRIGGER AS $$
BEGIN
  SELECT team_id INTO NEW.team_id FROM labels WHERE id = NEW.label_id;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_label_team_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_label_team_id();--> statement-breakpoint

CREATE OR REPLACE FUNCTION populate_issue_child_team_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT b.team_id INTO NEW.team_id
    FROM issues i JOIN boards b ON b.id = i.board_id
    WHERE i.id = NEW.issue_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_subscriber_team_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_event_team_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_coding_session_team_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_team_id();--> statement-breakpoint

CREATE OR REPLACE FUNCTION populate_issue_child_board_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.issue_id IS NOT NULL THEN
    SELECT board_id INTO NEW.board_id FROM issues WHERE id = NEW.issue_id
      FOR KEY SHARE;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_comment_board_id
  BEFORE INSERT ON comments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_attachment_board_id
  BEFORE INSERT ON attachments
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_event_board_id
  BEFORE INSERT ON issue_events
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_subscriber_board_id
  BEFORE INSERT ON issue_subscribers
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_coding_session_board_id
  BEFORE INSERT ON coding_sessions
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_issue_label_board_id
  BEFORE INSERT ON issue_labels
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_notification_board_id
  BEFORE INSERT ON notifications
  FOR EACH ROW EXECUTE FUNCTION populate_issue_child_board_id();
