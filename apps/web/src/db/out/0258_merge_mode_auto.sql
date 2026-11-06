-- EXP-1711 (rollout, the one-off step): the train is the only merge now, so every team (self-hosted included) runs on auto. An owner switches a team back to After approval in Settings.
UPDATE "teams" SET "merge_mode" = 'auto';
