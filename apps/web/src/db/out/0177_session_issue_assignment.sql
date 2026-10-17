ALTER TABLE "session_issues" ADD COLUMN "assigned_session_id" uuid;--> statement-breakpoint
ALTER TABLE "session_issues" ADD CONSTRAINT "session_issues_assigned_session_id_sessions_id_fk" FOREIGN KEY ("assigned_session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_session_issues_assigned_session" ON "session_issues" USING btree ("assigned_session_id");--> statement-breakpoint
-- EXP-1341: every root dispatches (agents included); older agent rows predate it.
UPDATE "sessions" SET "dispatch" = true WHERE "parent_id" IS NULL AND "kind" = 'agent' AND "dispatch" = false;
