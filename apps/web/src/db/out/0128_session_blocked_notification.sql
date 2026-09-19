ALTER TYPE "public"."notification_type" ADD VALUE 'session_blocked';--> statement-breakpoint
ALTER TABLE "notifications" ADD COLUMN "session_id" uuid;--> statement-breakpoint
ALTER TABLE "notifications" ADD CONSTRAINT "notifications_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_notifications_session" ON "notifications" USING btree ("session_id") WHERE session_id IS NOT NULL;