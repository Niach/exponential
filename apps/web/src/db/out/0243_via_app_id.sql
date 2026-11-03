ALTER TABLE "app_records" ADD COLUMN "via_app_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "via_app_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "via_app_id" uuid;--> statement-breakpoint
ALTER TABLE "app_records" ADD CONSTRAINT "app_records_via_app_id_apps_id_fk" FOREIGN KEY ("via_app_id") REFERENCES "public"."apps"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_via_app_id_apps_id_fk" FOREIGN KEY ("via_app_id") REFERENCES "public"."apps"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_via_app_id_apps_id_fk" FOREIGN KEY ("via_app_id") REFERENCES "public"."apps"("id") ON DELETE set null ON UPDATE no action;