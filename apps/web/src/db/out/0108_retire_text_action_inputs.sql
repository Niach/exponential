-- EXP-825: the free-text input kinds (`text`, `textarea`) are retired. What
-- the requester types when starting a run rides the start as `prompt` and
-- reaches the run as an "Additional instructions" section, so no action
-- declares a field for it anymore. Before dropping such definitions, the
-- FIRST one's placeholder (else its label) becomes the action's composer
-- hint (`prompt_placeholder`, added in 0107) so the action keeps telling the
-- requester what to type. Then every retired definition goes, keeping the
-- remaining picks (repo/board/pr/icon) in order. Idempotent: rows without a
-- retired def are untouched.
UPDATE actions
SET prompt_placeholder = LEFT(
  (SELECT COALESCE(NULLIF(def->>'placeholder', ''), def->>'label')
     FROM jsonb_array_elements(inputs) WITH ORDINALITY AS t(def, ord)
    WHERE def->>'type' IN ('text', 'textarea')
    ORDER BY ord
    LIMIT 1), 200)
WHERE prompt_placeholder IS NULL
  AND jsonb_typeof(inputs) = 'array'
  AND EXISTS (SELECT 1 FROM jsonb_array_elements(inputs) AS def
               WHERE def->>'type' IN ('text', 'textarea'));
--> statement-breakpoint
UPDATE actions
SET inputs = COALESCE(
  (SELECT jsonb_agg(def ORDER BY ord)
     FROM jsonb_array_elements(inputs) WITH ORDINALITY AS t(def, ord)
    WHERE def->>'type' NOT IN ('text', 'textarea')),
  '[]'::jsonb)
WHERE jsonb_typeof(inputs) = 'array'
  AND EXISTS (SELECT 1 FROM jsonb_array_elements(inputs) AS def
               WHERE def->>'type' IN ('text', 'textarea'));
