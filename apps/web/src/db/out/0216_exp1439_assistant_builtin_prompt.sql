-- #EXP-1439: the Assistant runs on Exponential's built-in prompt
-- (agent-settings.json `assistant.prompt`) and its instructions field is
-- gone. Each member's old text is posted ONCE into their newest Assistant
-- conversation (a consumed system row: shown, never a wake; the header is
-- `assistant.copy.legacyInstructions`, locked by assistant-prompt.test.ts),
-- and every Assistant is re-stamped so a live conversation shows the
-- Instructions changed hint and relaunches on the new prompt. The stored
-- text stays in `agents.instructions`, unread. The agent row, its
-- conversation, triggers and memory are untouched.
INSERT INTO "messages" ("conversation_id", "session_id", "root_id", "team_id", "shared", "owner_id", "author_kind", "body", "consumed_at")
SELECT c."id", s."id", s."root_id", s."team_id", s."shared", s."user_id", 'system',
  'Your Assistant now runs on Exponential''s built-in prompt, so its instructions field is gone. Your old instructions, to move into a team agent if you still need them:'
    || E'\n\n' || regexp_replace(btrim(a."instructions"), '^', '> ', 'gn'),
  now()
FROM "agents" a
JOIN LATERAL (
  SELECT * FROM "sessions" s
  WHERE s."agent_id" = a."id" AND s."user_id" = a."owner_id" AND s."parent_id" IS NULL
  ORDER BY s."created_at" DESC
  LIMIT 1
) s ON true
JOIN "conversations" c ON c."session_id" = s."id"
WHERE a."role" = 'assistant' AND btrim(a."instructions") <> '';
--> statement-breakpoint
UPDATE "agents" SET "instructions_updated_at" = now() WHERE "role" = 'assistant';
