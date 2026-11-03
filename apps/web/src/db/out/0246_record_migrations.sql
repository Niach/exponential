ALTER TABLE "app_records" ADD COLUMN "schema_version" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_state" varchar(16);--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_from" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_total" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_done" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_error" text;--> statement-breakpoint
ALTER TABLE "app_versions" ADD COLUMN "migration_by" text;--> statement-breakpoint
ALTER TABLE "app_versions" ADD CONSTRAINT "app_versions_migration_by_users_id_fk" FOREIGN KEY ("migration_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;