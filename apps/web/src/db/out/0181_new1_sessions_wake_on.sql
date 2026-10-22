ALTER TABLE "sessions" ADD COLUMN "wake_on" jsonb;--> statement-breakpoint
CREATE INDEX "idx_sessions_wake_on" ON "sessions" USING gin ("wake_on");