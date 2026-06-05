CREATE TABLE "issue_agent_state" (
	"issue_id" uuid PRIMARY KEY NOT NULL,
	"plan_text" jsonb,
	"question" jsonb,
	"question_asked_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "issue_agent_state" ADD CONSTRAINT "issue_agent_state_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
-- Backfill: seed a structured-state row for every in-flight agent issue, then
-- fill plan text + open question from the latest agent comments, so active
-- awaiting_approval/awaiting_answer issues keep their content in the new Plan
-- Panel. (Blocking step — do not defer.)
INSERT INTO "issue_agent_state" ("issue_id", "updated_at")
SELECT "id", now() FROM "issues" WHERE "agent_plan_state" IS NOT NULL
ON CONFLICT ("issue_id") DO NOTHING;--> statement-breakpoint
UPDATE "issue_agent_state" s
SET "plan_text" = p.body
FROM (
  SELECT DISTINCT ON (c."issue_id") c."issue_id", c."body" AS body
  FROM "comments" c WHERE c."kind" = 'plan'
  ORDER BY c."issue_id", c."created_at" DESC
) p
WHERE s."issue_id" = p."issue_id";--> statement-breakpoint
UPDATE "issue_agent_state" s
SET "question" = q.body, "question_asked_at" = q.created_at
FROM (
  SELECT DISTINCT ON (c."issue_id") c."issue_id", c."body" AS body, c."created_at" AS created_at
  FROM "comments" c
  WHERE c."kind" = 'question' AND c."answered_at" IS NULL
  ORDER BY c."issue_id", c."created_at" DESC
) q
JOIN "issues" i ON i."id" = q."issue_id"
WHERE s."issue_id" = q."issue_id" AND i."agent_plan_state" = 'awaiting_answer';