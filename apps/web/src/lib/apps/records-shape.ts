// eApps R3 (docs/spec/apps.md §3.4, §9.2): what the app shapes may carry to a
// member's device. Access `none` (the row, else the app's default) hides an app entirely (the apps,
// app-versions and app-records shapes drop it); a kind whose policy says
// `read: author` syncs to a non-owner only the rows they author, `read:
// owners` none at all (record-policy.json), so a private kind never reaches
// another member's device. The clauses are static SQL literals built from
// database-derived ids and validated kind names (RECORD_KIND_PATTERN), sorted
// so the same state yields byte-identical SQL (shape identity), and cached
// for 10 s like the membership sets (a policy or access change reaches the
// next renewal; the client re-syncs on the rotated handle).
import { sql } from "drizzle-orm"
import { RECORD_KIND_PATTERN } from "@exponential_app/eui/host"
import { sqlStringLiteral } from "@/lib/auth/shape-where"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"
import { hiddenAppIdsQuery } from "@/lib/apps/app-access-level"

export interface AppShapeRules {
  /** Apps the member may not see at all (effective level `none`). */
  hiddenAppIds: string[]
  /** Per app (of a team the member is NOT an owner of): kinds read by
   *  `owners` and by `author`. */
  privateKinds: { appId: string; owners: string[]; author: string[] }[]
}

const cache = new TtlPromiseCache<AppShapeRules>({ ttlMs: 10_000, maxEntries: 5_000 })

/** Forget every member's rules (a policy, access or role write; tests). */
export function invalidateAppShapeRules(): void {
  cache.clear()
}

async function queryRules(userId: string, teamIds: readonly string[]): Promise<AppShapeRules> {
  if (teamIds.length === 0) return { hiddenAppIds: [], privateKinds: [] }
  const { db } = await import(`@/db/connection`)
  const teams = sql`ARRAY[${sql.join(
    teamIds.map((t) => sql`${t}`),
    sql`, `
  )}]::uuid[]`
  // #FEED-242: the effective level (row, else creator/owner write, else the app's default).
  const hidden = await db.execute<{ app_id: string }>(hiddenAppIdsQuery(userId, teams))
  // eApps V1 (#VAPP-210): the published kinds plus a draft's new ones (`||`: the published definition wins).
  const kinds = await db.execute<{ app_id: string; kind: string; read: string }>(sql`
    SELECT a.id AS app_id, k.key AS kind, k.value->'policy'->>'read' AS read
    FROM apps a
    JOIN team_members m ON m.team_id = a.team_id AND m.user_id = ${userId} AND m.role <> 'owner'
    CROSS JOIN LATERAL jsonb_each((CASE WHEN jsonb_typeof(a.draft_package->'records') = 'object' THEN a.draft_package->'records' ELSE '{}'::jsonb END) || (CASE WHEN jsonb_typeof(a.package->'records') = 'object' THEN a.package->'records' ELSE '{}'::jsonb END)) k
    WHERE a.team_id = ANY(${teams}) AND a.deleted_at IS NULL AND k.value->'policy'->>'read' IN ('author', 'owners')`)
  const byApp = new Map<string, { owners: string[]; author: string[] }>()
  for (const row of kinds.rows) {
    if (!RECORD_KIND_PATTERN.test(row.kind)) continue
    const entry = byApp.get(row.app_id) ?? { owners: [], author: [] }
    ;(row.read === `owners` ? entry.owners : entry.author).push(row.kind)
    byApp.set(row.app_id, entry)
  }
  return {
    hiddenAppIds: hidden.rows.map((r) => r.app_id).sort(),
    privateKinds: [...byApp.entries()]
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([appId, k]) => ({ appId, owners: k.owners.sort(), author: k.author.sort() })),
  }
}

export function appShapeRules(userId: string, teamIds: readonly string[]): Promise<AppShapeRules> {
  return cache.get(`${userId}:${[...teamIds].sort().join(`,`)}`, () => queryRules(userId, teamIds))
}

const inList = (values: readonly string[]) => values.map(sqlStringLiteral).join(`,`)

/** `NOT ("<column>" IN (hidden apps))`, or null when none is hidden. */
export function hiddenAppsClause(column: string, rules: AppShapeRules): string | null {
  return rules.hiddenAppIds.length ? `NOT ("${column}" IN (${inList(rules.hiddenAppIds)}))` : null
}

/** The app-records clauses: hidden apps, then each private kind's rule. */
export function appRecordClauses(userId: string, rules: AppShapeRules): string[] {
  const out: string[] = []
  const hidden = hiddenAppsClause(`app_id`, rules)
  if (hidden) out.push(hidden)
  for (const k of rules.privateKinds) {
    const app = `"app_id" = ${sqlStringLiteral(k.appId)}`
    if (k.owners.length) out.push(`NOT (${app} AND "kind" IN (${inList(k.owners)}))`)
    // The author is `author_id`, else `created_by` (a legacy row); a row
    // with neither is an owner's only (NULL logic drops it).
    const me = sqlStringLiteral(userId)
    if (k.author.length) out.push(`NOT (${app} AND "kind" IN (${inList(k.author)}) AND NOT ("author_id" = ${me} OR ("author_id" IS NULL AND "created_by" = ${me})))`)
  }
  return out
}
