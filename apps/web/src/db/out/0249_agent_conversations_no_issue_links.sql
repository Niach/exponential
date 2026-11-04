-- EXP-1686 (session-kind.json `linksIssues`): an agent's conversation holds
-- no issue rows; issues reach it as messages. The rows already there go, no
-- status moves and no event card: their children and the threads the agents
-- started keep theirs.
DELETE FROM "session_issues" si
USING "sessions" s
WHERE s."id" = si."session_id" AND s."agent_id" IS NOT NULL;
