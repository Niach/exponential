-- F21: `sessions.send {clientMessageId}` (tRPC only), the sender's
-- idempotency key: a retried send naming it again returns the first row.
-- Server-only (outside the messages shape allowlist). The key leads the
-- index: the retry's lookup is by key + sender.
ALTER TABLE "messages" ADD COLUMN "client_message_id" varchar(64);--> statement-breakpoint
CREATE UNIQUE INDEX "uq_messages_client_message_id" ON "messages" USING btree ("client_message_id","conversation_id") WHERE client_message_id IS NOT NULL;--> statement-breakpoint
-- F30: plan mode is gone (S-3) and nothing writes `plan_mode` any more,
-- but the shipped daemon still launches a row that says true in plan mode.
UPDATE "sessions" SET "plan_mode" = false WHERE "plan_mode";
