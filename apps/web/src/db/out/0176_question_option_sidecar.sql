-- Release train 2026-10-16: #1080 (EXP-1322) made a question card's
-- `options` labels-only with the parallel `optionDescriptions` sidecar and
-- dropped the readers' tolerance for `{label, description}` objects. Rows
-- written by older device builds still hold objects, so new clients would
-- show such a question with no choices. Normalise them in place
-- (idempotent: only rows with at least one object option match).
UPDATE messages
SET card = jsonb_set(
  jsonb_set(card, '{options}', (
    SELECT jsonb_agg(CASE WHEN jsonb_typeof(o) = 'object' THEN COALESCE(o->'label', '""'::jsonb) ELSE o END)
    FROM jsonb_array_elements(card->'options') AS o
  )),
  '{optionDescriptions}', (
    SELECT jsonb_agg(CASE WHEN jsonb_typeof(o) = 'object' THEN COALESCE(o->'description', 'null'::jsonb) ELSE 'null'::jsonb END)
    FROM jsonb_array_elements(card->'options') AS o
  )
)
WHERE card->>'kind' = 'question'
  AND jsonb_typeof(card->'options') = 'array'
  AND EXISTS (
    SELECT 1 FROM jsonb_array_elements(card->'options') AS o WHERE jsonb_typeof(o) = 'object'
  );
