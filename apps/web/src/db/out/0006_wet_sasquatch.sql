CREATE TYPE "public"."project_type" AS ENUM('dev', 'tasks', 'feedback');--> statement-breakpoint
CREATE TYPE "public"."public_coding_visibility" AS ENUM('off', 'badge', 'live');--> statement-breakpoint
ALTER TABLE "projects" ALTER COLUMN "repository_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "type" "project_type" DEFAULT 'dev' NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "public_show_comments" boolean DEFAULT true NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "public_show_activity" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "public_show_coding" "public_coding_visibility" DEFAULT 'off' NOT NULL;--> statement-breakpoint
-- project_id denormalization: hand-edited from the generated NOT NULL adds —
-- add nullable, backfill from the owning issue, then tighten. The BEFORE
-- INSERT triggers in custom/0001_triggers.sql keep these populated going
-- forward.
ALTER TABLE "attachments" ADD COLUMN "project_id" uuid;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "project_id" uuid;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "project_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "project_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD COLUMN "project_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD COLUMN "project_id" uuid;--> statement-breakpoint
UPDATE "attachments" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
UPDATE "coding_sessions" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
UPDATE "comments" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
UPDATE "issue_events" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
UPDATE "issue_labels" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
UPDATE "issue_subscribers" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
ALTER TABLE "attachments" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "coding_sessions" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "comments" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_events" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_labels" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ALTER COLUMN "project_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "comments" ADD CONSTRAINT "comments_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD CONSTRAINT "issue_labels_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD CONSTRAINT "issue_subscribers_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_attachments_project" ON "attachments" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_coding_sessions_project" ON "coding_sessions" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_comments_project" ON "comments" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_issue_events_project" ON "issue_events" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_issue_labels_project" ON "issue_labels" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_issue_subscribers_project" ON "issue_subscribers" USING btree ("project_id");--> statement-breakpoint
CREATE INDEX "idx_projects_feedback" ON "projects" USING btree ("type") WHERE type = 'feedback';
