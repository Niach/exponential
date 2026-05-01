-- Denormalize workspace_id into issue_labels so the Electric shape filter
-- (workspace_id IN (...)) stays stable across label additions.
ALTER TABLE "issue_labels" ADD COLUMN "workspace_id" uuid;--> statement-breakpoint
UPDATE "issue_labels" SET "workspace_id" = "labels"."workspace_id" FROM "labels" WHERE "issue_labels"."label_id" = "labels"."id";--> statement-breakpoint
ALTER TABLE "issue_labels" ALTER COLUMN "workspace_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD CONSTRAINT "issue_labels_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issue_labels_workspace" ON "issue_labels" USING btree ("workspace_id");
