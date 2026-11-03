ALTER TABLE "attachments" ADD COLUMN "app_id" uuid;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_attachments_app" ON "attachments" USING btree ("app_id") WHERE app_id IS NOT NULL;