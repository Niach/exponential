ALTER TABLE "notifications" ADD COLUMN "project_id" uuid;--> statement-breakpoint
-- Hand-edited: backfill the denormalized project_id from the owning issue
-- (REV-109 — same pattern as 0006 for the six issue-child tables). Stays
-- nullable like issue_id; the BEFORE INSERT trigger (0001_triggers.sql §7)
-- keeps it populated going forward.
UPDATE "notifications" t SET "project_id" = i."project_id" FROM "issues" i WHERE i."id" = t."issue_id";--> statement-breakpoint
ALTER TABLE "notifications" ADD CONSTRAINT "notifications_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_notifications_project" ON "notifications" USING btree ("project_id");