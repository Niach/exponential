ALTER TABLE "devices" ADD COLUMN "live_session_ids" text[];--> statement-breakpoint
-- FEED-145: repair the watchdog's false `stuck` flags. The 30-minute rule flagged busy rows mid-turn (a long, report-free turn) and set needs_input, so working sessions read "needs input". Only that rule leaves `stuck` on a BUSY row (a dead process is flagged idle). Clear the flag and re-derive needs_input from the open question/plan cards, as `clearBlockedInTx` does. A row that really is wedged on a device without heartbeat claims gets flagged again after 30 minutes.
UPDATE "sessions" s SET
  "blocked" = NULL,
  "needs_input" = EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."answered_at" IS NULL AND m."card"->>'kind' IN ('question', 'plan')),
  "updated_at" = now()
WHERE s."status" IN ('running', 'idle') AND s."busy" = true AND s."blocked"->>'kind' = 'stuck';
