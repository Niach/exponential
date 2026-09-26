-- EXP-1082 §1 backfill: rows created before coding_sessions carried their
-- workflow membership (workflow_id / workflow_node_id / workflow_role) get it
-- from the facts that were recorded elsewhere, so the session tree groups
-- them like every newer row. Idempotent: only rows with no workflow yet.

-- A node's own run: the node records it (workflow_nodes.session_id).
UPDATE "coding_sessions" s
SET "workflow_id" = n."workflow_id",
    "workflow_node_id" = n."id",
    "workflow_role" = 'author'
FROM "workflow_nodes" n
WHERE n."session_id" = s."id"
  AND s."workflow_id" IS NULL;--> statement-breakpoint

-- A review run: named by its branch, exp/wf-<id8>-review-<IDENT>-r<n>, where
-- <id8> = the first 8 hex chars of the workflow id and <IDENT> = the node's
-- issue identifier.
UPDATE "coding_sessions" s
SET "workflow_id" = w."id",
    "workflow_node_id" = n."id",
    "workflow_role" = 'review'
FROM "workflows" w
JOIN "workflow_nodes" n ON n."workflow_id" = w."id"
JOIN "issues" i ON i."id" = n."issue_id"
WHERE s."workflow_id" IS NULL
  AND s."branch" ~ '^exp/wf-[0-9a-f]{8}-review-.+-r[0-9]+$'
  AND left(replace(w."id"::text, '-', ''), 8) = substring(s."branch" from '^exp/wf-([0-9a-f]{8})-review-')
  AND i."identifier" = substring(s."branch" from '^exp/wf-[0-9a-f]{8}-review-(.+)-r[0-9]+$');--> statement-breakpoint

-- A review run whose node no longer resolves (its issue left the workflow)
-- still belongs to the workflow.
UPDATE "coding_sessions" s
SET "workflow_id" = w."id",
    "workflow_role" = 'review'
FROM "workflows" w
WHERE s."workflow_id" IS NULL
  AND s."branch" ~ '^exp/wf-[0-9a-f]{8}-review-.+-r[0-9]+$'
  AND left(replace(w."id"::text, '-', ''), 8) = substring(s."branch" from '^exp/wf-([0-9a-f]{8})-review-');
