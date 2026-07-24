-- EXP-254: merge pre-existing duplicate labels (same team, case-insensitive
-- name) before the unique index lands. Keep the oldest label of each group,
-- repoint issue assignments at it, and drop the rest — issues that carried
-- both copies lose the duplicate's row via the issue_labels FK cascade.
WITH ranked AS (
	SELECT id, first_value(id) OVER (PARTITION BY team_id, lower(name) ORDER BY created_at, id) AS keep_id
	FROM labels
), dupes AS (
	SELECT id, keep_id FROM ranked WHERE id <> keep_id
)
UPDATE issue_labels il
SET label_id = d.keep_id
FROM dupes d
WHERE il.label_id = d.id
	AND NOT EXISTS (
		SELECT 1 FROM issue_labels x
		WHERE x.issue_id = il.issue_id AND x.label_id = d.keep_id
	);--> statement-breakpoint
DELETE FROM labels l
USING (
	SELECT id, first_value(id) OVER (PARTITION BY team_id, lower(name) ORDER BY created_at, id) AS keep_id
	FROM labels
) ranked
WHERE l.id = ranked.id AND ranked.id <> ranked.keep_id;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_labels_team_name_ci" ON "labels" USING btree ("team_id",lower("name"));
