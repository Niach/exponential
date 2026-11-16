CREATE TABLE "session_watches" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"session_id" uuid NOT NULL,
	"kind" varchar(16) NOT NULL,
	"subject_id" uuid,
	"status" varchar(32),
	"at" timestamp with time zone,
	"label" varchar(64) NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_session_watches_same" UNIQUE NULLS NOT DISTINCT("session_id","kind","subject_id","status","at")
);
--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "watching" jsonb;--> statement-breakpoint
ALTER TABLE "session_watches" ADD CONSTRAINT "session_watches_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_session_watches_session" ON "session_watches" USING btree ("session_id");--> statement-breakpoint
CREATE INDEX "idx_session_watches_subject" ON "session_watches" USING btree ("subject_id","kind");--> statement-breakpoint
CREATE INDEX "idx_session_watches_at" ON "session_watches" USING btree ("at") WHERE kind = 'at';--> statement-breakpoint
-- #NEW-49: the watched sessions move from the deprecated sessions.wake_on into
-- session_watches (live watchers, live watched sessions only), labelled like
-- lib/sessions/watches.ts sessionWatchLabel: the first linked issue's
-- identifier, else the title (clipped), else `a session`.
INSERT INTO "session_watches" ("session_id", "kind", "subject_id", "label")
SELECT w.id, 'session', t.id,
  coalesce(
    (SELECT i.identifier FROM session_issues si JOIN issues i ON i.id = si.issue_id
      WHERE si.session_id = t.id AND i.deleted_at IS NULL ORDER BY si.sort_order, si.id LIMIT 1),
    CASE WHEN length(btrim(coalesce(t.title, ''))) = 0 THEN NULL
      WHEN length(btrim(t.title)) > 32 THEN rtrim(left(btrim(t.title), 31)) || '…'
      ELSE btrim(t.title) END,
    'a session')
FROM sessions w
CROSS JOIN LATERAL jsonb_array_elements_text(coalesce(w.wake_on, '[]'::jsonb)) AS watched(id)
JOIN sessions t ON t.id::text = watched.id
WHERE w.status <> 'ended' AND t.status <> 'ended' AND t.id <> w.id
ON CONFLICT DO NOTHING;--> statement-breakpoint
UPDATE "sessions" s SET "watching" = sub.watching
FROM (
  SELECT session_id, jsonb_agg(jsonb_build_object('id', id, 'kind', kind, 'subjectId', subject_id, 'label', label) ORDER BY created_at, id) AS watching
  FROM session_watches GROUP BY session_id
) sub
WHERE sub.session_id = s.id;
