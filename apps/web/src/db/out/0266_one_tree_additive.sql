ALTER TABLE "agents" ADD COLUMN "defaults" jsonb DEFAULT '{}'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "apps" ADD COLUMN "agent_id" uuid;--> statement-breakpoint
ALTER TABLE "attachments" ADD COLUMN "session_id" uuid;--> statement-breakpoint
ALTER TABLE "messages" ADD COLUMN "session_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "digest" jsonb;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "lead_assistant_id" uuid;--> statement-breakpoint
ALTER TABLE "apps" ADD CONSTRAINT "apps_agent_id_agents_id_fk" FOREIGN KEY ("agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "attachments" ADD CONSTRAINT "attachments_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "teams" ADD CONSTRAINT "teams_lead_assistant_id_agents_id_fk" FOREIGN KEY ("lead_assistant_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_apps_agent" ON "apps" USING btree ("agent_id") WHERE agent_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_attachments_session" ON "attachments" USING btree ("session_id") WHERE session_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_messages_session_created" ON "messages" USING btree ("session_id","created_at");--> statement-breakpoint
-- #EXP-1730 phase A (one-tree model): the new columns stay in step with the
-- legacy ones whichever side a writer names, so shipped clients and old
-- server paths keep working while new code reads the new model. Phase B
-- drops these with the legacy columns. Distinct names from the older
-- functions, so an older build's boot SQL never replaces them.

-- messages.session_id <-> messages.conversation_id (fires before
-- populate_message_session_mirrors: trigger names run in order).
CREATE OR REPLACE FUNCTION populate_message_session_id()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.conversation_id IS NULL AND NEW.session_id IS NOT NULL THEN
    SELECT c.id INTO NEW.conversation_id FROM conversations c WHERE c.session_id = NEW.session_id;
  ELSIF NEW.conversation_id IS NOT NULL THEN
    SELECT c.session_id INTO NEW.session_id FROM conversations c WHERE c.id = NEW.conversation_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint

CREATE OR REPLACE TRIGGER populate_message_session_id
  BEFORE INSERT ON messages
  FOR EACH ROW EXECUTE FUNCTION populate_message_session_id();
--> statement-breakpoint

-- sessions.digest = its conversation's digest. A digest-only write is no
-- edit of the session (updated_at stays: the watchdog and the lists read it).
CREATE OR REPLACE FUNCTION mirror_conversation_digest()
RETURNS TRIGGER AS $$
DECLARE
  prev text := current_setting('exponential.preserve_timestamps', true);
BEGIN
  PERFORM set_config('exponential.preserve_timestamps', 'on', true);
  UPDATE sessions SET digest = NEW.digest
    WHERE id = NEW.session_id AND digest IS DISTINCT FROM NEW.digest;
  PERFORM set_config('exponential.preserve_timestamps', coalesce(prev, ''), true);
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint

CREATE OR REPLACE TRIGGER mirror_conversation_digest
  AFTER INSERT OR UPDATE OF digest ON conversations
  FOR EACH ROW
  WHEN (NEW.session_id IS NOT NULL AND NEW.digest IS NOT NULL)
  EXECUTE FUNCTION mirror_conversation_digest();
--> statement-breakpoint

-- agents.defaults <-> agents.device_id/cli/account/model: a write that
-- changes only `defaults` (or an insert without a device) leads with the
-- object; otherwise the columns lead. Either way both end up equal.
CREATE OR REPLACE FUNCTION sync_agent_defaults()
RETURNS TRIGGER AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND (NEW.device_id IS NULL OR NEW.defaults <> '{}'::jsonb))
    OR (TG_OP = 'UPDATE' AND NEW.defaults IS DISTINCT FROM OLD.defaults
      AND NEW.device_id IS NOT DISTINCT FROM OLD.device_id
      AND NEW.cli IS NOT DISTINCT FROM OLD.cli
      AND NEW.account IS NOT DISTINCT FROM OLD.account
      AND NEW.model IS NOT DISTINCT FROM OLD.model) THEN
    IF NEW.defaults ? 'deviceId' THEN NEW.device_id := coalesce(NEW.defaults->>'deviceId', ''); END IF;
    IF NEW.defaults ? 'cli' THEN NEW.cli := NEW.defaults->>'cli'; END IF;
    IF NEW.defaults ? 'account' THEN NEW.account := NEW.defaults->>'account'; END IF;
    IF NEW.defaults ? 'model' THEN NEW.model := NEW.defaults->>'model'; END IF;
    NEW.device_id := coalesce(NEW.device_id, '');
  END IF;
  NEW.defaults := jsonb_build_object(
    'deviceId', nullif(NEW.device_id, ''),
    'cli', NEW.cli,
    'account', NEW.account,
    'model', NEW.model);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
--> statement-breakpoint

CREATE OR REPLACE TRIGGER sync_agent_defaults
  BEFORE INSERT OR UPDATE ON agents
  FOR EACH ROW EXECUTE FUNCTION sync_agent_defaults();
--> statement-breakpoint
-- Backfills: bookkeeping, not edits (no updated_at bump; transaction-local).
SELECT set_config('exponential.preserve_timestamps', 'on', true);
--> statement-breakpoint
UPDATE "messages" m SET "session_id" = c."session_id"
FROM "conversations" c
WHERE c."id" = m."conversation_id" AND c."session_id" IS NOT NULL AND m."session_id" IS NULL;
--> statement-breakpoint
UPDATE "sessions" s SET "digest" = c."digest"
FROM "conversations" c
WHERE c."session_id" = s."id" AND c."digest" IS NOT NULL AND s."digest" IS DISTINCT FROM c."digest";
--> statement-breakpoint
UPDATE "agents" SET "defaults" = jsonb_build_object(
  'deviceId', nullif("device_id", ''), 'cli', "cli", 'account', "account", 'model', "model");
--> statement-breakpoint
-- Every app gets its agent (the app's name, icon and color; no
-- instructions or triggers yet): owned by its creator, else its publisher,
-- else the team's first owner; on the device its newest editor session ran
-- on, else the owner's newest session device in the team. An app with no
-- possible owner stays standalone.
CREATE TABLE "exp1730_app_agents" AS
SELECT gen_random_uuid() AS "agent_id", a."id" AS "app_id", a."team_id", a."name", a."icon", a."color", o."owner_id",
  coalesce(
    (SELECT s."device_id" FROM "sessions" s WHERE s."app_id" = a."id" ORDER BY s."created_at" DESC LIMIT 1),
    (SELECT s."device_id" FROM "sessions" s WHERE s."team_id" = a."team_id" AND s."user_id" = o."owner_id" ORDER BY s."created_at" DESC LIMIT 1),
    '') AS "device_id"
FROM "apps" a
CROSS JOIN LATERAL (
  SELECT coalesce(a."created_by", a."published_by",
    (SELECT tm."user_id" FROM "team_members" tm WHERE tm."team_id" = a."team_id" AND tm."role" = 'owner' ORDER BY tm."created_at", tm."user_id" LIMIT 1)) AS "owner_id"
) o
WHERE a."agent_id" IS NULL AND o."owner_id" IS NOT NULL;
--> statement-breakpoint
INSERT INTO "agents" ("id", "team_id", "name", "icon", "color", "device_id", "owner_id", "role")
SELECT "agent_id", "team_id", "name", "icon", "color", "device_id", "owner_id", 'agent' FROM "exp1730_app_agents";
--> statement-breakpoint
UPDATE "apps" a SET "agent_id" = m."agent_id" FROM "exp1730_app_agents" m WHERE m."app_id" = a."id";
--> statement-breakpoint
DROP TABLE "exp1730_app_agents";
--> statement-breakpoint
-- Results: every Guide picture's attachment row names the session that
-- filed it (the entry's writer, else the root).
UPDATE "attachments" a SET "session_id" = r."session_id"
FROM (
  SELECT DISTINCT ON ((e->>'attachmentId')::uuid) (e->>'attachmentId')::uuid AS "attachment_id",
    coalesce(w."id", s."id") AS "session_id"
  FROM "sessions" s
  CROSS JOIN LATERAL jsonb_array_elements(s."guide") e
  LEFT JOIN "sessions" w ON w."id"::text = e->>'sessionId'
  WHERE s."guide" IS NOT NULL AND jsonb_typeof(s."guide") = 'array'
    AND e->>'attachmentId' ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
) r
WHERE a."id" = r."attachment_id" AND a."session_id" IS NULL;
--> statement-breakpoint
SELECT set_config('exponential.preserve_timestamps', '', true);
