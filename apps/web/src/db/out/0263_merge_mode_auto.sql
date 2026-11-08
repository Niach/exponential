-- #EXP-1711 (rollout, the one-off step): the merge train is the only merge
-- now, so every team (self-hosted included) runs on auto. An owner switches a
-- team back to After approval in Settings. A new team takes the column
-- default (`auto`).
UPDATE "teams" SET "merge_mode" = 'auto';
