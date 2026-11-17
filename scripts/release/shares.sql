SELECT d.device_id AS "deviceId", t.team_id AS "teamId", tm.slug AS "teamSlug",
  json_agg(json_build_object('id', d.id, 'userId', d.user_id, 'email', u.email, 'createdAt', d.created_at)
    ORDER BY d.created_at, d.id) AS rows
FROM devices d
CROSS JOIN LATERAL unnest(d.shared_team_ids) AS t(team_id)
LEFT JOIN users u ON u.id = d.user_id
LEFT JOIN teams tm ON tm.id = t.team_id
GROUP BY d.device_id, t.team_id, tm.slug
HAVING count(*) > 1
ORDER BY d.device_id, t.team_id
