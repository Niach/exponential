ALTER TABLE "apps" DROP CONSTRAINT "apps_team_id_slug_unique";--> statement-breakpoint
ALTER TABLE "apps" DROP CONSTRAINT "apps_created_by_users_id_fk";
--> statement-breakpoint
ALTER TABLE "apps" ALTER COLUMN "created_by" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "apps" ADD CONSTRAINT "apps_created_by_users_id_fk" FOREIGN KEY ("created_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_apps_team_slug" ON "apps" USING btree ("team_id","slug") WHERE deleted_at IS NULL;