ALTER TABLE "attachments" ADD COLUMN "workspace_id" uuid;--> statement-breakpoint
UPDATE "attachments" SET "workspace_id" = (
  SELECT "projects"."workspace_id"
  FROM "issues"
  JOIN "projects" ON "projects"."id" = "issues"."project_id"
  WHERE "issues"."id" = "attachments"."issue_id"
) WHERE "workspace_id" IS NULL;--> statement-breakpoint
ALTER TABLE "attachments" ALTER COLUMN "workspace_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_attachments_workspace" ON "attachments" USING btree ("workspace_id");
