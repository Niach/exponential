-- NEW-8 step 4: a message names its conversation only. The mirror trigger
-- turns conversation-keyed here, BEFORE the column goes (the boot's custom
-- SQL re-applies the same body), so no insert ever runs the old function
-- against a dropped column. The pending and ask-mirror indexes re-key on
-- `conversation_id` (one conversation per session: the same rows).
CREATE OR REPLACE FUNCTION populate_message_session_mirrors()
RETURNS TRIGGER AS $$
DECLARE
  src record;
BEGIN
  SELECT s.root_id, s.team_id, s.shared, s.user_id INTO src
  FROM conversations c
  JOIN sessions s ON s.id = c.session_id
  WHERE c.id = NEW.conversation_id;
  IF FOUND THEN
    NEW.root_id := src.root_id;
    NEW.team_id := src.team_id;
    NEW.shared := src.shared;
    NEW.owner_id := src.user_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;--> statement-breakpoint
ALTER TABLE "messages" DROP CONSTRAINT "messages_session_id_sessions_id_fk";
--> statement-breakpoint
DROP INDEX "idx_messages_session_created";--> statement-breakpoint
DROP INDEX "idx_messages_pending";--> statement-breakpoint
DROP INDEX "uq_messages_ask_mirror";--> statement-breakpoint
CREATE INDEX "idx_messages_pending" ON "messages" USING btree ("conversation_id") WHERE author_kind = 'user' AND consumed_at IS NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uq_messages_ask_mirror" ON "messages" USING btree ("conversation_id",(card->>'messageId')) WHERE card->>'kind' = 'ask';--> statement-breakpoint
ALTER TABLE "messages" DROP COLUMN "session_id";