-- EXP-825: the free-text input kinds (`text`, `textarea`) are retired. What
-- the requester types when starting a run rides the start as `prompt` and
-- reaches the run as an "Additional instructions" section, so no action
-- declares a field for it anymore. Drop every such definition from stored
-- actions, keeping the remaining picks (repo/board/pr/icon) in order.
-- Idempotent: rows without a retired def are untouched (no updated_at bump).
UPDATE actions
SET inputs = COALESCE(
  (SELECT jsonb_agg(def ORDER BY ord)
     FROM jsonb_array_elements(inputs) WITH ORDINALITY AS t(def, ord)
    WHERE def->>'type' NOT IN ('text', 'textarea')),
  '[]'::jsonb)
WHERE jsonb_typeof(inputs) = 'array'
  AND EXISTS (SELECT 1 FROM jsonb_array_elements(inputs) AS def
               WHERE def->>'type' IN ('text', 'textarea'));
