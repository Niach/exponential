-- #EXP-1408: bus lines written before this change carried a PRIVATE chat's text into conversations whose readers may not see it (a shared one, or another member's: `askHidden`). Each becomes the neutral `privateText` (the lead + the session id only): a relayed turn reply (the row the reply's own transaction wrote, same instant), an end report (its card's summary too) and a PR line (its card drops `pullRequestId`). A session's own `sessions_send` input stays: its author wrote it there.
UPDATE "messages" m SET
  "body" = 'A private chat replied.' || E'\n' || 'Read it with exponential_sessions({op: "get", id: "' || o."id" || '"}) if your account may see it.',
  "updated_at" = now()
FROM "sessions" o, "sessions" l
WHERE o."id" = m."author_session_id" AND l."id" = m."session_id" AND m."session_id" <> m."author_session_id"
  AND NOT o."shared" AND (l."shared" OR l."user_id" <> o."user_id")
  AND m."author_kind" = 'agent' AND m."card" IS NULL
  AND m."body" NOT LIKE 'A private chat replied.%'
  AND EXISTS (
    SELECT 1 FROM "messages" r
    WHERE r."session_id" = o."id" AND r."author_kind" = 'agent' AND r."card" IS NULL
      AND r."author_session_id" IS NULL AND r."created_at" = m."created_at"
  );--> statement-breakpoint
UPDATE "messages" m SET
  "body" = 'A private chat finished.' || E'\n' || 'Read it with exponential_sessions({op: "get", id: "' || o."id" || '"}) if your account may see it.',
  "card" = m."card" || jsonb_build_object('summary', 'A private chat finished.'),
  "updated_at" = now()
FROM "sessions" o, "sessions" l
WHERE o."id" = m."author_session_id" AND l."id" = m."session_id" AND m."session_id" <> m."author_session_id"
  AND NOT o."shared" AND (l."shared" OR l."user_id" <> o."user_id")
  AND m."card"->>'kind' = 'report'
  AND m."body" NOT LIKE 'A private chat finished.%';--> statement-breakpoint
UPDATE "messages" m SET
  "body" = 'A private chat opened a pull request.' || E'\n' || 'Read it with exponential_sessions({op: "get", id: "' || o."id" || '"}) if your account may see it.',
  "card" = m."card" - 'pullRequestId',
  "updated_at" = now()
FROM "sessions" o, "sessions" l
WHERE o."id" = m."author_session_id" AND l."id" = m."session_id" AND m."session_id" <> m."author_session_id"
  AND NOT o."shared" AND (l."shared" OR l."user_id" <> o."user_id")
  AND m."card"->>'kind' = 'event' AND m."card"->>'event' = 'pr_opened'
  AND m."body" NOT LIKE 'A private chat opened a pull request.%';
