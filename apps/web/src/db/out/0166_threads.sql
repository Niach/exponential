CREATE TYPE "public"."thread_item_kind" AS ENUM('issue', 'run');--> statement-breakpoint
CREATE TYPE "public"."thread_item_source" AS ENUM('user', 'agent');--> statement-breakpoint
CREATE TABLE "thread_items" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"thread_id" uuid NOT NULL,
	"kind" "thread_item_kind" NOT NULL,
	"issue_id" uuid,
	"session_id" uuid,
	"team_id" uuid NOT NULL,
	"board_id" uuid,
	"board_deleted_at" timestamp with time zone,
	"board_archived_at" timestamp with time zone,
	"added_by" text,
	"added_by_session_id" uuid,
	"source" "thread_item_source" DEFAULT 'user' NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "thread_items_kind_target_check" CHECK ((kind = 'issue' AND issue_id IS NOT NULL AND session_id IS NULL) OR (kind = 'run' AND session_id IS NOT NULL AND issue_id IS NULL))
);
--> statement-breakpoint
CREATE TABLE "threads" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"title" varchar(255),
	"created_by" text,
	"results" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_thread_id_threads_id_fk" FOREIGN KEY ("thread_id") REFERENCES "public"."threads"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_added_by_users_id_fk" FOREIGN KEY ("added_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "thread_items" ADD CONSTRAINT "thread_items_added_by_session_id_coding_sessions_id_fk" FOREIGN KEY ("added_by_session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "threads" ADD CONSTRAINT "threads_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "threads" ADD CONSTRAINT "threads_created_by_users_id_fk" FOREIGN KEY ("created_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_thread_items_thread_issue" ON "thread_items" USING btree ("thread_id","issue_id") WHERE issue_id IS NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_thread_items_session" ON "thread_items" USING btree ("session_id") WHERE session_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_thread_items_thread" ON "thread_items" USING btree ("thread_id");--> statement-breakpoint
CREATE INDEX "idx_thread_items_issue" ON "thread_items" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_thread_items_session" ON "thread_items" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "idx_thread_items_team" ON "thread_items" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_thread_items_board" ON "thread_items" USING btree ("board_id");--> statement-breakpoint
CREATE INDEX "idx_thread_items_added_by_session" ON "thread_items" USING btree ("added_by_session_id");--> statement-breakpoint
CREATE INDEX "idx_threads_team" ON "threads" USING btree ("team_id");--> statement-breakpoint
-- EXP-1284 backfill: every existing run joins a thread, so lists can show
-- threads only. A resume chain (resumed_from_id) shares ONE thread per team
-- (a resume never carries across teams), keyed by its earliest member's id.
-- Issue items: every issue a member covered (issue_id + batch_issue_ids) that
-- still exists in the thread's team, with the board context the populate
-- triggers would derive (they are applied at boot, after this runs). The
-- Guide comes from the newest member holding one (the resume carry).
CREATE TEMP TABLE exp1284_chain AS
WITH RECURSIVE chain AS (
  SELECT id, id AS root FROM coding_sessions WHERE resumed_from_id IS NULL
  UNION ALL
  SELECT c.id, chain.root FROM coding_sessions c JOIN chain ON c.resumed_from_id = chain.id
)
SELECT
  cs.id AS session_id,
  cs.team_id,
  cs.user_id,
  cs.issue_id,
  cs.batch_issue_ids,
  cs.results,
  cs.created_at,
  cs.updated_at,
  first_value(cs.id) OVER (PARTITION BY chain.root, cs.team_id ORDER BY cs.created_at, cs.id) AS thread_id,
  row_number() OVER (PARTITION BY chain.root, cs.team_id ORDER BY cs.created_at, cs.id) AS ord
FROM chain JOIN coding_sessions cs ON cs.id = chain.id;
--> statement-breakpoint
INSERT INTO threads (id, team_id, title, created_by, results, created_at, updated_at)
SELECT
  m.thread_id,
  m.team_id,
  NULL,
  (SELECT f.user_id FROM exp1284_chain f WHERE f.thread_id = m.thread_id AND f.ord = 1),
  (SELECT r.results FROM exp1284_chain r
     WHERE r.thread_id = m.thread_id AND jsonb_typeof(r.results) = 'array' AND jsonb_array_length(r.results) > 0
     ORDER BY r.created_at DESC, r.session_id DESC LIMIT 1),
  min(m.created_at),
  max(m.updated_at)
FROM exp1284_chain m
GROUP BY m.thread_id, m.team_id;
--> statement-breakpoint
INSERT INTO thread_items (thread_id, kind, session_id, team_id, added_by, source, sort_order, created_at, updated_at)
SELECT m.thread_id, 'run', m.session_id, m.team_id, m.user_id, 'user', m.ord, m.created_at, m.created_at
FROM exp1284_chain m;
--> statement-breakpoint
INSERT INTO thread_items (thread_id, kind, issue_id, team_id, board_id, board_deleted_at, board_archived_at, added_by, source, sort_order, created_at, updated_at)
SELECT DISTINCT ON (covered.thread_id, i.id)
  covered.thread_id, 'issue', i.id, b.team_id, i.board_id, b.deleted_at, b.archived_at,
  covered.user_id, 'user', covered.ord * 100 + covered.pos, covered.created_at, covered.created_at
FROM (
  SELECT m.thread_id, m.team_id, m.user_id, m.ord, m.created_at, m.issue_id::text AS issue_ref, 0::bigint AS pos
  FROM exp1284_chain m WHERE m.issue_id IS NOT NULL
  UNION ALL
  SELECT m.thread_id, m.team_id, m.user_id, m.ord, m.created_at, e.value, e.pos
  FROM exp1284_chain m,
    LATERAL jsonb_array_elements_text(CASE WHEN jsonb_typeof(m.batch_issue_ids) = 'array' THEN m.batch_issue_ids ELSE '[]'::jsonb END)
      WITH ORDINALITY AS e(value, pos)
) covered
JOIN issues i ON i.id::text = covered.issue_ref
JOIN boards b ON b.id = i.board_id AND b.team_id = covered.team_id
ORDER BY covered.thread_id, i.id, covered.ord, covered.pos;
--> statement-breakpoint
DROP TABLE exp1284_chain;
