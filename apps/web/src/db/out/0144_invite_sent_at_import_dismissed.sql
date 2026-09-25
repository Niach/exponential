ALTER TABLE "import_jobs" ADD COLUMN "dismissed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "team_invites" ADD COLUMN "sent_at" timestamp with time zone;--> statement-breakpoint
-- EXP-1076: every invite that existed before this column was, by definition,
-- a link that went out. NULL from here on means "roster row, never invited"
-- (the import's placeholder members), so the backfill must not leave old rows
-- reading as uninvited.
UPDATE "team_invites" SET "sent_at" = "created_at" WHERE "sent_at" IS NULL;
