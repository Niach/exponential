-- A session's conversation mirrors its visibility (`shared`, `owner_id`):
-- agent device moves and account-deletion hand-overs re-stamped the session
-- tree but not the conversation, so re-sync the drifted rows.
UPDATE conversations c
SET shared = s.shared, owner_id = s.user_id
FROM sessions s
WHERE c.session_id = s.id
  AND (c.shared IS DISTINCT FROM s.shared OR c.owner_id IS DISTINCT FROM s.user_id);
--> statement-breakpoint
-- An agent's chats are titled after it (`sessionTitle` reads `title` only):
-- roots migrated with a `name` but no `title` take the name.
UPDATE sessions
SET title = name
WHERE agent_id IS NOT NULL
  AND parent_id IS NULL
  AND coalesce(trim(title), '') = ''
  AND coalesce(trim(name), '') <> '';
