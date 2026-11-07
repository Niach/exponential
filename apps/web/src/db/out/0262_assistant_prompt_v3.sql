-- #EXP-1717: the Assistant's built-in prompt v3 (agent-settings.json
-- `assistant.prompt`: routes a team's work to its coordinator). Every
-- Assistant is re-stamped so a live conversation shows the Instructions
-- changed hint and relaunches on the new prompt.
UPDATE "agents" SET "instructions_updated_at" = now() WHERE "role" = 'assistant';
