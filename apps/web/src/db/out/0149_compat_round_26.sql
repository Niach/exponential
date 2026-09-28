ALTER TABLE "workflows" DROP COLUMN "gate";--> statement-breakpoint
-- Compat round 26 (EXP-1125 item 10): every stored `workflows.launch` is
-- rewritten to the canonical {agent, model, strongModel, account?} shape,
-- EXACTLY what `normalizeWorkflowLaunch` (web lib/workflow-launch.ts, Rust
-- coding::workflows::launch, iOS WorkflowConfig.swift, Android
-- WorkflowRows.kt) folded the row into before this release, so every client
-- reads the same launch after the folds of the retired keys were deleted:
-- - agent: the string `codex`, else `claude`;
-- - model: the stored (trimmed, non-blank) string, else the agent's contract
--   `workflowLaunch` default (claude opus, codex gpt-5.6-sol);
-- - strongModel: the stored string, else the FIRST set of the retired pins
--   reviewModel, riskModel, contractModel, integrationModel (that order),
--   else the agent's default strong model (claude fable, codex gpt-5.6-luna);
-- - account: the stored string when non-blank, else absent;
-- - subagentModel, effort, maxParallel and any other key are dropped.
-- Idempotent: a canonical row folds to itself.
UPDATE "workflows" AS w
SET "launch" = jsonb_strip_nulls(jsonb_build_object(
  'agent', f.agent,
  'model', COALESCE(f.model, CASE WHEN f.agent = 'codex' THEN 'gpt-5.6-sol' ELSE 'opus' END),
  'strongModel', COALESCE(
    f.strong_model, f.review_model, f.risk_model, f.contract_model, f.integration_model,
    CASE WHEN f.agent = 'codex' THEN 'gpt-5.6-luna' ELSE 'fable' END
  ),
  'account', f.account
))
FROM (
  SELECT
    "id",
    CASE WHEN jsonb_typeof("launch" -> 'agent') = 'string' AND "launch" ->> 'agent' = 'codex'
      THEN 'codex' ELSE 'claude' END AS agent,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'model') = 'string' THEN "launch" ->> 'model' END, '^\s+|\s+$', '', 'g'), '') AS model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'strongModel') = 'string' THEN "launch" ->> 'strongModel' END, '^\s+|\s+$', '', 'g'), '') AS strong_model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'reviewModel') = 'string' THEN "launch" ->> 'reviewModel' END, '^\s+|\s+$', '', 'g'), '') AS review_model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'riskModel') = 'string' THEN "launch" ->> 'riskModel' END, '^\s+|\s+$', '', 'g'), '') AS risk_model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'contractModel') = 'string' THEN "launch" ->> 'contractModel' END, '^\s+|\s+$', '', 'g'), '') AS contract_model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'integrationModel') = 'string' THEN "launch" ->> 'integrationModel' END, '^\s+|\s+$', '', 'g'), '') AS integration_model,
    NULLIF(regexp_replace(CASE WHEN jsonb_typeof("launch" -> 'account') = 'string' THEN "launch" ->> 'account' END, '^\s+|\s+$', '', 'g'), '') AS account
  FROM "workflows"
) AS f
WHERE f."id" = w."id";--> statement-breakpoint
-- Compat round 26 (EXP-1125 item 20): `waiting` left the node vocabulary
-- (EXP-1065 holds are `running` + a note). Zero rows on prod and staging when
-- audited; defensive for any self-host that still has one.
UPDATE "workflow_nodes" SET "state" = 'running' WHERE "state" = 'waiting';