ALTER TABLE "projects" ADD COLUMN "is_public" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "icon" text;--> statement-breakpoint
CREATE INDEX "idx_projects_public" ON "projects" USING btree ("is_public") WHERE is_public;--> statement-breakpoint
-- Backfill in the SAME migration: the public-scope query keys on is_public the
-- moment the new server boots, so pre-existing feedback boards must already
-- carry it when the column lands.
UPDATE "projects" SET "is_public" = true WHERE "type" = 'feedback';
