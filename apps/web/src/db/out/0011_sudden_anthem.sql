ALTER TABLE "projects" ADD COLUMN "deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "is_protected" boolean DEFAULT false NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_projects_deleted" ON "projects" USING btree ("deleted_at") WHERE deleted_at IS NOT NULL;