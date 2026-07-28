-- EXP-286 rebrand: the paid tiers Pro and Business collapsed into the single
-- `team` tier (PlanTier = free | team | unlimited). teams.comp_tier is plain
-- text, so this is a pure data migration — parseCompTier no longer accepts
-- the legacy values (clean cut, no alias), so any comped row must be
-- rewritten or its comp silently stops applying. Idempotent.
UPDATE "teams" SET "comp_tier" = 'team' WHERE "comp_tier" IN ('pro', 'business');
