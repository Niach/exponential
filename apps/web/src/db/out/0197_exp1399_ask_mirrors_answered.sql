-- #EXP-1399: an `ask` mirror (a question/plan card mirrored into a listener's conversation) now closes with its origin card (`closeAskMirrors`). Close every mirror whose origin was already answered or superseded, so lists and Docks reading the mirror stop waiting on it.
UPDATE "messages" m SET
  "answered_at" = o."answered_at",
  "updated_at" = now()
FROM "messages" o
WHERE m."answered_at" IS NULL
  AND m."card"->>'kind' = 'ask'
  AND o."id"::text = m."card"->>'messageId'
  AND o."answered_at" IS NOT NULL;
