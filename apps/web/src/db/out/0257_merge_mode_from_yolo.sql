-- EXP-1709 fix-up: while MERGE_TRAIN is off, a team's merge mode mirrors today's yolo switch (yolo = auto, else approval) so the merge status pill reads true. The rollout's flip of every team to auto is a separate one-off step (EXP-1711).
UPDATE "teams" SET "merge_mode" = CASE WHEN "yolo_mode" THEN 'auto' ELSE 'approval' END;
