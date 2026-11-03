ALTER TABLE "apps" ADD COLUMN "published_by" text;--> statement-breakpoint
ALTER TABLE "apps" ADD COLUMN "automation_state" jsonb DEFAULT '{}'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "apps" ADD CONSTRAINT "apps_published_by_users_id_fk" FOREIGN KEY ("published_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
-- #VAPP-220: the running version's writer stands in as its publisher until the next Publish.
UPDATE "apps" AS a SET "published_by" = v."created_by" FROM "app_versions" AS v WHERE v."app_id" = a."id" AND v."number" = a."published_version" AND a."published_version" > 0;
