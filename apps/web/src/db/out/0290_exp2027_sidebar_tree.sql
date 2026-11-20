ALTER TABLE "sidebar_layout" DROP CONSTRAINT "sidebar_layout_section_check";--> statement-breakpoint
ALTER TABLE "sidebar_layout" ADD CONSTRAINT "sidebar_layout_section_check" CHECK (section IN ('team', 'board', 'agent', 'app'));--> statement-breakpoint
-- #EXP-2027 (sidebar-tree.json): fold each member's Boards, Agents and Apps
-- sections into ONE `team` list per team without moving anything: every
-- target the member never placed gets an entry in its default order (boards
-- and agents by sort_order, apps by name) after the placed ones, and the
-- three top-level blocks keep their order (boards, agents, apps). Rows
-- inside a folder keep their sort_order (it is relative to the folder).
WITH pairs AS (
  SELECT DISTINCT user_id, team_id FROM sidebar_layout WHERE section IN ('board', 'agent', 'app')
), defaults AS (
  SELECT p.user_id, p.team_id, 'board' AS section, b.id AS target_id,
         row_number() OVER (PARTITION BY p.user_id, p.team_id ORDER BY b.sort_order, b.id) AS position
  FROM pairs p JOIN boards b ON b.team_id = p.team_id
  WHERE b.deleted_at IS NULL AND b.archived_at IS NULL
  UNION ALL
  SELECT p.user_id, p.team_id, 'agent', a.id,
         row_number() OVER (PARTITION BY p.user_id, p.team_id ORDER BY a.sort_order, a.id)
  FROM pairs p JOIN agents a ON a.team_id = p.team_id
  WHERE a.role = 'agent'
  UNION ALL
  SELECT p.user_id, p.team_id, 'app', x.id,
         row_number() OVER (PARTITION BY p.user_id, p.team_id ORDER BY lower(x.name), x.id)
  FROM pairs p JOIN apps x ON x.team_id = p.team_id
  WHERE x.deleted_at IS NULL
)
INSERT INTO sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
SELECT d.user_id, d.team_id, d.section, 'item', d.target_id, NULL, 50000 + d.position
FROM defaults d
WHERE NOT EXISTS (
  SELECT 1 FROM sidebar_layout l WHERE l.user_id = d.user_id AND l.section = d.section AND l.target_id = d.target_id
);
--> statement-breakpoint
UPDATE sidebar_layout
SET sort_order = CASE section WHEN 'board' THEN 0 WHEN 'agent' THEN 100000 ELSE 200000 END + sort_order
WHERE section IN ('board', 'agent', 'app') AND parent_id IS NULL;
--> statement-breakpoint
UPDATE sidebar_layout SET section = 'team', updated_at = now() WHERE section IN ('board', 'agent', 'app');
