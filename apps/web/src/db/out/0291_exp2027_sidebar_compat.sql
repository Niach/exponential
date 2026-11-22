ALTER TABLE "sidebar_layout" DROP CONSTRAINT "sidebar_layout_kind_check";--> statement-breakpoint
ALTER TABLE "sidebar_layout" ADD CONSTRAINT "sidebar_layout_kind_check" CHECK ((kind = 'folder' AND parent_id IS NULL AND (target_id IS NULL OR section <> 'team')) OR (kind = 'item' AND target_id IS NOT NULL));--> statement-breakpoint
-- #EXP-2027 (sidebar-compat.json): each member's per-kind sections are COPIED
-- into one `team` list per team and stay as they are, so a client that still
-- reads them keeps its folders and order. Every per-kind folder gets a team
-- copy and links it in `target_id`; the three top-level blocks keep their
-- order (boards, agents, apps); a target never placed gets a team entry in
-- its default order after the placed ones. A member who already has a team
-- list for that team is left alone.
WITH pairs AS (
  SELECT DISTINCT l.user_id, l.team_id FROM sidebar_layout l
  WHERE l.section IN ('board', 'agent', 'app')
    AND NOT EXISTS (
      SELECT 1 FROM sidebar_layout t WHERE t.user_id = l.user_id AND t.team_id = l.team_id AND t.section = 'team'
    )
), folders AS MATERIALIZED (
  SELECT l.id, gen_random_uuid() AS team_folder_id, l.user_id, l.team_id, l.name,
         CASE l.section WHEN 'board' THEN 0 WHEN 'agent' THEN 100000 ELSE 200000 END + l.sort_order AS sort_order
  FROM sidebar_layout l JOIN pairs p ON p.user_id = l.user_id AND p.team_id = l.team_id
  WHERE l.kind = 'folder' AND l.section IN ('board', 'agent', 'app')
), team_folders AS (
  INSERT INTO sidebar_layout (id, user_id, team_id, section, kind, name, sort_order)
  SELECT team_folder_id, user_id, team_id, 'team', 'folder', name, sort_order FROM folders
), team_items AS (
  INSERT INTO sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
  SELECT l.user_id, l.team_id, 'team', 'item', l.target_id, f.team_folder_id,
         CASE WHEN f.id IS NULL THEN CASE l.section WHEN 'board' THEN 0 WHEN 'agent' THEN 100000 ELSE 200000 END + l.sort_order
              ELSE l.sort_order END
  FROM sidebar_layout l
  JOIN pairs p ON p.user_id = l.user_id AND p.team_id = l.team_id
  LEFT JOIN folders f ON f.id = l.parent_id
  WHERE l.kind = 'item' AND l.section IN ('board', 'agent', 'app') AND l.target_id IS NOT NULL
  ON CONFLICT (user_id, section, target_id) WHERE target_id IS NOT NULL DO NOTHING
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
), team_defaults AS (
  INSERT INTO sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
  SELECT d.user_id, d.team_id, 'team', 'item', d.target_id, NULL,
         CASE d.section WHEN 'board' THEN 0 WHEN 'agent' THEN 100000 ELSE 200000 END + 50000 + d.position
  FROM defaults d
  WHERE NOT EXISTS (
    SELECT 1 FROM sidebar_layout l WHERE l.user_id = d.user_id AND l.section = d.section AND l.target_id = d.target_id
  )
  ON CONFLICT (user_id, section, target_id) WHERE target_id IS NOT NULL DO NOTHING
)
UPDATE sidebar_layout l SET target_id = f.team_folder_id, updated_at = now()
FROM folders f WHERE l.id = f.id;
