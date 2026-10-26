import { sql } from "drizzle-orm"
import type { db as Db } from "@/db/connection"

// FEED-149: the release train's read-only check. FEED-148 allows ONE sharing
// row per (machine, team); rows from before it can still pair up. Each pair
// is listed with every sharing row, first sharer (the host) first, so an
// owner of the team can drop the others (`devices.dropShare`, the Team
// devices row's Remove).

export type DuplicateShare = {
  deviceId: string
  teamId: string
  teamSlug: string | null
  rows: { id: string; userId: string; email: string | null; createdAt: string }[]
}

export async function duplicateDeviceShares(database: Pick<typeof Db, `execute`>): Promise<DuplicateShare[]> {
  const result = await database.execute(sql`
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
  `)
  return result.rows as unknown as DuplicateShare[]
}
