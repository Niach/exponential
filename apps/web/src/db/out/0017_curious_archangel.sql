-- Unwrap the legacy jsonb `{ text }` envelope into a plain markdown text column.
-- The USING cast is required (jsonb→text has no implicit cast) and preserves data.
ALTER TABLE "comments" ALTER COLUMN "body" SET DATA TYPE text USING "body"->>'text';--> statement-breakpoint
ALTER TABLE "issues" ALTER COLUMN "description" SET DATA TYPE text USING "description"->>'text';