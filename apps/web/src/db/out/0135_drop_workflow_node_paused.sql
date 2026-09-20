-- `wfNodeState: paused` is gone with the node budgets that were its only
-- producer (0134). The column is a documented varchar, so nothing in the
-- schema pins the vocabulary — but a row left saying `paused` would render
-- its raw wire word on every client and take none of the exits a failed node
-- takes. There is exactly one exit that fits: `failed`, which offers Retry
-- and Skip, the two things a paused node offered.
--
-- Idempotent, and a no-op on any database where no budget ever tripped.
UPDATE "workflow_nodes" SET "state" = 'failed' WHERE "state" = 'paused';
