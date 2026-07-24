-- EXP-254: merge pre-existing duplicate labels (same team, case-insensitive
-- name) before the unique index lands. Keep the oldest label of each group,
-- repoint issue assignments at it, and drop the rest — issues that carried
-- both copies lose the duplicate's row via the issue_labels FK cascade.
-- Per (issue, label group) keep exactly ONE assignment row first — preferring
-- the keep label's own row — so the repoint UPDATE can never collide on the
-- issue_labels (issue_id, label_id) PK when an issue carries several
-- duplicates of the same group.
WITH ranked AS (
	SELECT id, first_value(id) OVER (PARTITION BY team_id, lower(name) ORDER BY created_at, id) AS keep_id
	FROM labels
), mapped AS (
	SELECT il.issue_id, il.label_id,
		row_number() OVER (
			PARTITION BY il.issue_id, r.keep_id
			ORDER BY (il.label_id = r.keep_id) DESC, il.label_id
		) AS rn
	FROM issue_labels il
	JOIN ranked r ON r.id = il.label_id
)
DELETE FROM issue_labels il
USING mapped m
WHERE il.issue_id = m.issue_id AND il.label_id = m.label_id AND m.rn > 1;--> statement-breakpoint
WITH ranked AS (
	SELECT id, first_value(id) OVER (PARTITION BY team_id, lower(name) ORDER BY created_at, id) AS keep_id
	FROM labels
), dupes AS (
	SELECT id, keep_id FROM ranked WHERE id <> keep_id
)
UPDATE issue_labels il
SET label_id = d.keep_id
FROM dupes d
WHERE il.label_id = d.id;--> statement-breakpoint
DELETE FROM labels l
USING (
	SELECT id, first_value(id) OVER (PARTITION BY team_id, lower(name) ORDER BY created_at, id) AS keep_id
	FROM labels
) ranked
WHERE l.id = ranked.id AND ranked.id <> ranked.keep_id;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_labels_team_name_ci" ON "labels" USING btree ("team_id",lower("name"));
