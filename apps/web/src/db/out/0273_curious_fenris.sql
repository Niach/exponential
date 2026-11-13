CREATE TYPE "public"."repository_provider" AS ENUM('github', 'ecode');--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "provider_updated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "provider" "repository_provider" DEFAULT 'github' NOT NULL;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "remote_url" text;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "provider_config" jsonb DEFAULT '{}'::jsonb NOT NULL;--> statement-breakpoint
-- #EXP-1614: backfill. `installation_id` moves into `provider_config` (the column stays one release, #NEW-14); `github_updated_at` becomes `provider_updated_at` (the old column stays one release, still written).
UPDATE "repositories" SET "provider_config" = jsonb_build_object('installationId', "installation_id") WHERE "installation_id" IS NOT NULL;--> statement-breakpoint
UPDATE "pull_requests" SET "provider_updated_at" = "github_updated_at" WHERE "github_updated_at" IS NOT NULL;
