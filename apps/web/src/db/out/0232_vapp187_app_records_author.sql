ALTER TABLE "app_records" ADD COLUMN "author_id" text;--> statement-breakpoint
ALTER TABLE "app_records" ADD COLUMN "updated_by" text;--> statement-breakpoint
ALTER TABLE "app_records" ADD CONSTRAINT "app_records_author_id_users_id_fk" FOREIGN KEY ("author_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_records" ADD CONSTRAINT "app_records_updated_by_users_id_fk" FOREIGN KEY ("updated_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
UPDATE "app_records" SET "author_id" = "created_by", "updated_by" = "created_by" WHERE "author_id" IS NULL;