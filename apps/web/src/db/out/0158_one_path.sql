-- SLOP-4: one path. A widget submission IS an issue; the standalone helpdesk
-- (support_threads / support_messages) is gone. Data first, then the drops.
--
-- Every migration runs inside ONE transaction (drizzle's migrator), so the
-- comment_source enum cannot take `ADD VALUE 'reporter'` and USE the value in
-- the same transaction (Postgres refuses "unsafe use of new value"). Both
-- enums are therefore RECREATED through a text hop, the way drizzle-kit
-- itself does for notification_type below.
CREATE TYPE "public"."comment_audience" AS ENUM('team', 'reporter');--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "source" DROP DEFAULT;--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "source" SET DATA TYPE text;--> statement-breakpoint
DROP TYPE "public"."comment_source";--> statement-breakpoint
CREATE TYPE "public"."comment_source" AS ENUM('user', 'mcp', 'reporter');--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "source" SET DATA TYPE "public"."comment_source" USING "source"::"public"."comment_source";--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "source" SET DEFAULT 'user';--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "author_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "audience" "comment_audience" DEFAULT 'team' NOT NULL;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "email_delivery_id" uuid;--> statement-breakpoint
ALTER TABLE "comments" ADD CONSTRAINT "comments_email_delivery_id_email_deliveries_id_fk" FOREIGN KEY ("email_delivery_id") REFERENCES "public"."email_deliveries"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_submissions" ADD COLUMN "last_reporter_seen_at" timestamp with time zone;--> statement-breakpoint
-- The issue-less `support_reply` rows pointed at the Support surface, which no
-- longer exists; the per-type email opt-out moves to the new type.
DELETE FROM "notifications" WHERE "type" = 'support_reply';--> statement-breakpoint
UPDATE "user_notification_prefs"
SET "type_prefs" = ("type_prefs" - 'support_reply') || jsonb_build_object('reporter_reply', "type_prefs"->'support_reply')
WHERE "type_prefs" ? 'support_reply';--> statement-breakpoint
ALTER TABLE "notifications" ALTER COLUMN "type" SET DATA TYPE text;--> statement-breakpoint
DROP TYPE "public"."notification_type";--> statement-breakpoint
CREATE TYPE "public"."notification_type" AS ENUM('issue_assigned', 'issue_comment', 'issue_status_changed', 'issue_mention', 'issue_created', 'pr_opened', 'pr_merged', 'reporter_reply', 'agent_message', 'session_blocked');--> statement-breakpoint
ALTER TABLE "notifications" ALTER COLUMN "type" SET DATA TYPE "public"."notification_type" USING "type"::"public"."notification_type";--> statement-breakpoint
-- The data step. Each support thread becomes an issue on its widget's board;
-- a team whose widget has no board, or a thread without a widget, gets ONE
-- "Support" board. Inbound messages become reporter comments, outbound
-- public ones member comments with audience `reporter`, internal notes team
-- comments. Reporter text is UNTRUSTED plain text: it is escaped into safe
-- GFM the way apps/web lib/reporter-text.ts does (the SQL twin of
-- escapeReporterText), so no renderer needs a plain-text branch.
CREATE OR REPLACE FUNCTION pg_temp.slop4_escape(body text) RETURNS text AS $$
DECLARE
  out text := coalesce(body, '');
BEGIN
  -- Backslash first, then every markdown special that can open structure or
  -- fire a mention/ref/autolink.
  out := regexp_replace(out, '([\\`*_\[\]<>#~|@&])', '\\\1', 'g');
  -- A dot that starts a domain label (`example.com`): kills GFM autolinks.
  out := regexp_replace(out, '\.([A-Za-z0-9])', '\\.\1', 'g');
  -- Line starts: an indented code block loses its indent, list markers and
  -- setext underlines are escaped.
  out := regexp_replace(out, '^(\t| {4,})[ \t]*', '', 'gn');
  out := regexp_replace(out, '^([ \t]*)([-+])', '\1\\\2', 'gn');
  out := regexp_replace(out, '^([ \t]*[0-9]+)([.)])', '\1\\\2', 'gn');
  out := regexp_replace(out, '^([ \t]*)(=)', '\1\\\2', 'gn');
  RETURN out;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint
CREATE TEMP TABLE slop4_thread_issue (thread_id uuid PRIMARY KEY, issue_id uuid NOT NULL, board_id uuid NOT NULL, team_id uuid NOT NULL);--> statement-breakpoint
DO $$
DECLARE
  t RECORD;
  support_board uuid;
  prefix_pick text;
  slug_pick text;
BEGIN
  -- 1. Every team that needs a Support board: a thread whose widget has no
  --    live board (or no widget at all), or a board-less widget config.
  FOR t IN
    SELECT DISTINCT team_id FROM (
      SELECT st.team_id
      FROM support_threads st
      LEFT JOIN widget_submissions ws ON ws.support_thread_id = st.id
      LEFT JOIN widget_configs wc ON wc.id = ws.widget_config_id
      LEFT JOIN boards b ON b.id = wc.board_id AND b.deleted_at IS NULL AND b.archived_at IS NULL
      WHERE b.id IS NULL
      UNION ALL
      SELECT team_id FROM widget_configs WHERE board_id IS NULL
    ) needs
  LOOP
    SELECT id INTO support_board FROM boards
    WHERE team_id = t.team_id AND deleted_at IS NULL AND archived_at IS NULL
      AND (lower(name) = 'support' OR slug = 'support')
    ORDER BY created_at LIMIT 1;
    IF support_board IS NULL THEN
      SELECT p INTO prefix_pick FROM unnest(ARRAY['SUP', 'SUPP', 'HELP', 'TKT', 'SPRT']) AS p
      WHERE NOT EXISTS (SELECT 1 FROM boards WHERE team_id = t.team_id AND prefix = p)
      LIMIT 1;
      IF prefix_pick IS NULL THEN
        prefix_pick := upper(left(md5(gen_random_uuid()::text), 4));
      END IF;
      SELECT s INTO slug_pick FROM unnest(ARRAY['support', 'support-inbox', 'support-' || left(gen_random_uuid()::text, 8)]) AS s
      WHERE NOT EXISTS (SELECT 1 FROM boards WHERE team_id = t.team_id AND slug = s)
      LIMIT 1;
      INSERT INTO boards (team_id, name, slug, prefix)
      VALUES (t.team_id, 'Support', slug_pick, prefix_pick)
      RETURNING id INTO support_board;
    END IF;
    UPDATE widget_configs SET board_id = support_board WHERE team_id = t.team_id AND board_id IS NULL;
  END LOOP;
END $$;--> statement-breakpoint
-- The per-team Support board lookup for the thread loop: the board named
-- Support (created above or pre-existing).
CREATE TEMP TABLE slop4_support_board (team_id uuid PRIMARY KEY, board_id uuid NOT NULL);--> statement-breakpoint
INSERT INTO slop4_support_board (team_id, board_id)
SELECT DISTINCT ON (b.team_id) b.team_id, b.id FROM boards b
WHERE b.deleted_at IS NULL AND b.archived_at IS NULL AND (lower(b.name) = 'support' OR b.slug LIKE 'support%')
ORDER BY b.team_id, (lower(b.name) = 'support') DESC, b.created_at;--> statement-breakpoint
DO $$
DECLARE
  thread RECORD;
  target_board uuid;
  new_issue uuid;
  first_inbound text;
  resolved boolean;
  sub RECORD;
BEGIN
  -- 2. Threads → issues (oldest first, so identifiers follow arrival order).
  FOR thread IN
    SELECT st.*, wc.board_id AS widget_board_id, ws.id AS submission_id
    FROM support_threads st
    LEFT JOIN widget_submissions ws ON ws.support_thread_id = st.id
    LEFT JOIN widget_configs wc ON wc.id = ws.widget_config_id
    ORDER BY st.created_at, st.id
  LOOP
    SELECT b.id INTO target_board FROM boards b
    WHERE b.id = thread.widget_board_id AND b.deleted_at IS NULL AND b.archived_at IS NULL;
    IF target_board IS NULL THEN
      SELECT board_id INTO target_board FROM slop4_support_board WHERE team_id = thread.team_id;
    END IF;
    IF target_board IS NULL THEN
      RAISE EXCEPTION 'SLOP-4 migration: no Support board for team %', thread.team_id;
    END IF;

    SELECT body INTO first_inbound FROM support_messages
    WHERE thread_id = thread.id AND direction = 'inbound'
    ORDER BY created_at, id LIMIT 1;
    resolved := thread.status = 'resolved';

    INSERT INTO issues (board_id, team_id, title, description, status, status_id, priority, creator_id, source, completed_at, created_at, updated_at)
    VALUES (
      target_board, thread.team_id, left(pg_temp.slop4_escape(thread.title), 500),
      NULLIF(pg_temp.slop4_escape(first_inbound), ''),
      CASE WHEN resolved THEN 'done'::issue_status ELSE 'backlog'::issue_status END,
      NULL, 'none', NULL, 'widget',
      CASE WHEN resolved THEN thread.updated_at ELSE NULL END,
      thread.created_at, thread.updated_at
    )
    RETURNING id INTO new_issue;
    INSERT INTO slop4_thread_issue (thread_id, issue_id, board_id, team_id)
    VALUES (thread.id, new_issue, target_board, thread.team_id);

    INSERT INTO issue_events (issue_id, team_id, board_id, actor_user_id, type, payload, created_at, updated_at)
    VALUES (new_issue, thread.team_id, target_board, NULL, 'created',
      jsonb_build_object('status', CASE WHEN resolved THEN 'done' ELSE 'backlog' END, 'statusId', NULL, 'priority', 'none', 'source', 'widget'),
      thread.created_at, thread.created_at);

    -- The reporter keeps the one-way resolution mail on a later close.
    INSERT INTO issue_subscribers (issue_id, user_id, email, team_id, board_id, source, unsubscribed)
    VALUES (new_issue, NULL, thread.reporter_email, thread.team_id, target_board, 'widget_reporter', false)
    ON CONFLICT DO NOTHING;

    -- 3. The submission row is the issue's card + the reporter identity.
    IF thread.submission_id IS NOT NULL THEN
      UPDATE widget_submissions
      SET issue_id = new_issue,
          reporter_email = coalesce(reporter_email, thread.reporter_email),
          reporter_name = coalesce(reporter_name, thread.reporter_name),
          last_reporter_seen_at = thread.last_reporter_seen_at,
          resolved_notified_at = CASE WHEN resolved THEN thread.updated_at ELSE resolved_notified_at END
      WHERE id = thread.submission_id;
    ELSE
      INSERT INTO widget_submissions (widget_config_id, issue_id, reporter_email, reporter_name, last_reporter_seen_at, resolved_notified_at, created_at, updated_at)
      VALUES (NULL, new_issue, thread.reporter_email, thread.reporter_name, thread.last_reporter_seen_at,
        CASE WHEN resolved THEN thread.updated_at ELSE NULL END, thread.created_at, thread.updated_at);
    END IF;

    -- 4. Messages → comments. The opening inbound message IS the description,
    --    so it is skipped; every later message becomes a comment.
    INSERT INTO comments (issue_id, team_id, board_id, author_id, parent_id, source, audience, email_delivery_id, body, created_at, updated_at)
    SELECT new_issue, thread.team_id, target_board,
      CASE WHEN m.direction = 'inbound' THEN NULL ELSE m.author_user_id END,
      NULL,
      CASE WHEN m.direction = 'inbound' THEN 'reporter'::comment_source ELSE 'user'::comment_source END,
      CASE WHEN m.direction = 'inbound' OR m.visibility = 'public' THEN 'reporter'::comment_audience ELSE 'team'::comment_audience END,
      m.email_delivery_id,
      CASE WHEN m.direction = 'inbound' THEN pg_temp.slop4_escape(m.body) ELSE m.body END,
      m.created_at, m.created_at
    FROM support_messages m
    WHERE m.thread_id = thread.id
      AND m.id <> (SELECT id FROM support_messages WHERE thread_id = thread.id AND direction = 'inbound' ORDER BY created_at, id LIMIT 1)
    ORDER BY m.created_at, m.id;

    -- 5. An escalated issue stays linked: a `related` reference row
    --    (canonical direction: issue_id < related_issue_id).
    IF thread.linked_issue_id IS NOT NULL AND EXISTS (SELECT 1 FROM issues WHERE id = thread.linked_issue_id) THEN
      INSERT INTO issue_relations (issue_id, related_issue_id, type, source, team_id, board_id)
      SELECT least(new_issue, thread.linked_issue_id), greatest(new_issue, thread.linked_issue_id), 'related', 'reference', thread.team_id,
        (SELECT board_id FROM issues WHERE id = least(new_issue, thread.linked_issue_id))
      ON CONFLICT DO NOTHING;
    END IF;
  END LOOP;
END $$;--> statement-breakpoint
-- A submission row that never got an issue (defensive: the old schema allowed
-- neither anchor) cannot survive the NOT NULL below.
DELETE FROM "widget_submissions" WHERE "issue_id" IS NULL;--> statement-breakpoint
DROP TABLE slop4_thread_issue;--> statement-breakpoint
DROP TABLE slop4_support_board;--> statement-breakpoint
DROP FUNCTION pg_temp.slop4_escape(text);--> statement-breakpoint
ALTER TABLE "support_messages" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "support_threads" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "widget_configs" DROP CONSTRAINT "widget_configs_board_id_boards_id_fk";
--> statement-breakpoint
ALTER TABLE "widget_submissions" DROP CONSTRAINT "widget_submissions_support_thread_id_support_threads_id_fk";
--> statement-breakpoint
DROP INDEX "idx_widget_submissions_thread";--> statement-breakpoint
ALTER TABLE "widget_submissions" DROP COLUMN "support_thread_id";--> statement-breakpoint
DROP TABLE "support_messages" CASCADE;--> statement-breakpoint
DROP TABLE "support_threads" CASCADE;--> statement-breakpoint
ALTER TABLE "widget_configs" ALTER COLUMN "board_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "widget_submissions" ALTER COLUMN "issue_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "teams" DROP COLUMN "helpdesk_enabled";
