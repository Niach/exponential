// #VAPP-220 (eApps A1, docs/spec/apps.md §7.1): the EXECUTOR of an app's
// automations. One trigger (a schedule tick, a committed record write, a
// team event) runs ONE binding through the apps broker (`runBrokerTool`:
// the same records service, grants and upstream credentials as an exposed
// tool) on the PUBLISHED package, as the automation's SPONSOR (@exp/emcp
// automations.json: the publisher, or a member an owner-publisher named).
// Each run, and each refusal, is one `mcp_calls` row (`client_id =
// automation:<name>`), its writes read "via <app>". Runs never chain: a run
// executes inside `runInAutomation`, so what it writes or emits fires
// nothing. A paused automation (its sponsor gone or read-only) notifies the
// app's owners once per pause.
import { and, eq, gt, isNull, like, sql } from "drizzle-orm"
import {
  automationClientId,
  automationExpose,
  automationTool,
  matchAutomations,
  rateAllows,
  resolveSponsor,
  validateAutomations,
  type Automation,
  type SponsorResult,
  type SponsorState,
} from "@exp/emcp/automations"
import { argsHash, callStatus, type CallStatus } from "@exp/emcp/audit"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { levelOf } from "@/lib/apps/app-access-level"
import { appAccess, apps, events, mcpCalls, teamMembers, teams, users, type App, type Event } from "@/db/schema"
import { latestCronMatch, parseCron } from "@/lib/agents/cron"
import { actingAgentSlug } from "@/lib/apps/agent-handoff"
import { automationsOf, runInAutomation, type RecordTriggerOp } from "@/lib/apps/automation-scope"
import { classifyAppToolResponse, runBrokerTool, type BrokerApp, type BrokerTool } from "@/lib/emcp/apps-broker"
import { recordMcpCalls } from "@/lib/emcp/audit"
import { loadRoutedServer } from "@/lib/emcp/upstream/device-call"
import { notifyAutomationPaused } from "@/lib/integrations/notifications"
import { appBaseUrl } from "@/lib/notification-email-policy"

export type AutomationApp = Pick<App, `id` | `slug` | `name` | `icon` | `color` | `teamId` | `package` | `grantedTools` | `publishedBy` | `createdBy` | `automationState`>

const APP_COLUMNS = {
  id: apps.id,
  slug: apps.slug,
  name: apps.name,
  icon: apps.icon,
  color: apps.color,
  teamId: apps.teamId,
  package: apps.package,
  grantedTools: apps.grantedTools,
  publishedBy: apps.publishedBy,
  createdBy: apps.createdBy,
  automationState: apps.automationState,
}

/** The published package's automations that validate (a stale entry is
 *  never run). */
export function publishedAutomations(pkg: unknown): Automation[] {
  const list = automationsOf(pkg)
  const records = (pkg as { records?: unknown } | null)?.records
  const upstreams = (pkg as { upstreams?: unknown } | null)?.upstreams
  const ctx = {
    kinds: records && typeof records === `object` ? Object.keys(records) : [],
    upstreams: Array.isArray(upstreams) ? upstreams.filter((u): u is string => typeof u === `string`) : [],
  }
  const broken = new Set(validateAutomations(list, ctx).map((e) => e.index))
  if (broken.has(null)) return []
  return list.filter((_, i) => !broken.has(i)) as Automation[]
}

/** C23: the automation's sponsor among the team's current members. */
export async function sponsorOf(app: Pick<App, `id` | `teamId` | `publishedBy`>, automation: Pick<Automation, `sponsor`>): Promise<SponsorResult> {
  const members = await db
    .select({ id: users.id, email: users.email, role: teamMembers.role, level: appAccess.level })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .leftJoin(appAccess, and(eq(appAccess.appId, app.id), eq(appAccess.userId, teamMembers.userId)))
    .where(eq(teamMembers.teamId, app.teamId))
  // #FEED-242: no row = creator/owner write, else the app's default.
  const [policy] = await db.select({ createdBy: apps.createdBy, defaultAccess: apps.defaultAccess }).from(apps).where(eq(apps.id, app.id)).limit(1)
  return resolveSponsor({
    declared: automation.sponsor ?? null,
    publishedBy: app.publishedBy,
    members: members.map((m) => ({ id: m.id, email: m.email, role: m.role, level: policy ? levelOf(policy, m.id, m.role, m.level) : (m.level ?? `write`) })),
  })
}

/** Merges `patch` into one automation's server state. */
async function patchState(appId: string, name: string, patch: Record<string, unknown>) {
  await db
    .update(apps)
    .set({
      automationState: sql`${apps.automationState} || jsonb_build_object(${name}::text, coalesce(${apps.automationState} -> ${name}::text, '{}'::jsonb) || ${JSON.stringify(patch)}::jsonb)`,
    })
    .where(eq(apps.id, appId))
}

export async function auditAutomation(app: AutomationApp, automation: Automation, userId: string | null, status: CallStatus, error: string | null, startedAt: number, input: unknown) {
  if (!userId) return
  // #VAPP-254: an upstream run's row names the REAL server (the app's mapping).
  const run = automation.run as { upstream?: string; tool?: string }
  const real = run.upstream ? await loadRoutedServer(app.teamId, run.upstream, app.id) : null
  await recordMcpCalls([
    {
      teamId: app.teamId,
      userId,
      clientId: automationClientId(automation.name),
      appId: app.id,
      tool: (real ? `${real.name}/${run.tool}` : automationTool(automation.run)).slice(0, 128),
      status,
      error,
      latencyMs: Math.max(0, Math.floor(Date.now() - startedAt)),
      argsHash: argsHash(input),
      sessionId: null,
      target: null,
    },
  ])
}

/** Runs of the app's automations in the last minute (the rate cap).
 *  #VAPP-257: only runs that RAN count; the cap's own `rateLimited`
 *  refusal rows never keep the window full. */
export async function runsLastMinute(appId: string): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(mcpCalls)
    .where(
      and(
        eq(mcpCalls.appId, appId),
        like(mcpCalls.clientId, `automation:%`),
        gt(mcpCalls.createdAt, new Date(Date.now() - 60_000)),
        sql`${mcpCalls.error} IS DISTINCT FROM 'rateLimited'`
      )
    )
  return row?.n ?? 0
}

export interface AutomationInput {
  record?: Record<string, unknown>
  event?: Record<string, unknown>
  firedAt: string
}

/** #VAPP-236: an agent run's ref = a record trigger's record. */
function handoffRef(automation: Automation, input: AutomationInput): { kind: string; id: string } | null {
  if (!(`record` in automation.on) || typeof input.record?.id !== `string`) return null
  return { kind: automation.on.record.kind, id: input.record.id }
}

/** One run: the rate cap, the sponsor, then the binding through the broker. */
export async function runAutomation(app: AutomationApp, automation: Automation, input: AutomationInput): Promise<CallStatus> {
  const startedAt = Date.now()
  const fallbackUser = app.publishedBy ?? app.createdBy
  if (!rateAllows(await runsLastMinute(app.id))) {
    await auditAutomation(app, automation, fallbackUser, `denied`, `rateLimited`, startedAt, input)
    return `denied`
  }
  const sponsor = await sponsorOf(app, automation)
  const state = app.automationState?.[automation.name]
  if (`paused` in sponsor) {
    await auditAutomation(app, automation, fallbackUser, `denied`, sponsor.paused, startedAt, input)
    if (state?.paused !== sponsor.paused) {
      await patchState(app.id, automation.name, { paused: sponsor.paused, pausedAt: new Date().toISOString() })
      await notifyPaused(app, automation.name, sponsor.paused)
    }
    return `denied`
  }
  if (state?.paused) await patchState(app.id, automation.name, { paused: null, pausedAt: null })
  const [user] = await db.select().from(users).where(eq(users.id, sponsor.userId)).limit(1)
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, app.teamId)).limit(1)
  if (!user || !team) return `denied`
  const pkg = (app.package ?? {}) as { upstreams?: unknown; queries?: unknown }
  const brokerApp: BrokerApp = {
    id: app.id,
    slug: app.slug,
    name: app.name,
    icon: app.icon,
    color: app.color,
    teamId: app.teamId,
    grantedTools: app.grantedTools,
    upstreams: Array.isArray(pkg.upstreams) ? pkg.upstreams.filter((u): u is string => typeof u === `string`) : [],
    level: `write`,
    queries: pkg.queries && typeof pkg.queries === `object` && !Array.isArray(pkg.queries) ? (pkg.queries as Record<string, unknown>) : undefined,
  }
  const tool: BrokerTool = { name: automation.name, app: brokerApp, expose: automationExpose(automation) }
  const ctx = {
    user,
    request: new Request(`${appBaseUrl()}/api/mcp/apps`),
    teamId: app.teamId,
    teamSlug: team.slug,
    handoff: { source: `automation ${automation.name}`, ref: handoffRef(automation, input) },
  }
  let status: CallStatus
  let error: string | null
  try {
    const result = await runInAutomation({ appId: app.id, name: automation.name }, () => runBrokerTool(ctx, tool, input))
    const classified = classifyAppToolResponse({ result })
    status = callStatus(classified.outcome)
    error = classified.error
  } catch (err) {
    console.error(`[automations] ${app.slug}/${automation.name} failed:`, err)
    status = `failed`
    error = `toolError`
  }
  await auditAutomation(app, automation, sponsor.userId, status, error, startedAt, input)
  return status
}

async function owners(teamId: string): Promise<string[]> {
  const rows = await db.select({ id: teamMembers.userId }).from(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.role, `owner`)))
  return rows.map((r) => r.id)
}

async function notifyPaused(app: AutomationApp, name: string, reason: SponsorState) {
  await notifyAutomationPaused({ teamId: app.teamId, ownerIds: await owners(app.teamId), appName: app.name, appSlug: app.slug, automation: name, reason })
}

export async function loadAutomationApp(appId: string): Promise<AutomationApp | null> {
  const [row] = await db
    .select(APP_COLUMNS)
    .from(apps)
    .where(and(eq(apps.id, appId), isNull(apps.deletedAt)))
    .limit(1)
  return row ?? null
}

/** A committed record write of the app (`afterRecordWrite`). */
export async function fireRecordAutomations(
  appId: string,
  kind: string,
  op: RecordTriggerOp,
  record: Record<string, unknown>,
  causingSessionId: string | null = null
): Promise<number> {
  const app = await loadAutomationApp(appId)
  if (!app) return 0
  const list = publishedAutomations(app.package)
  const byAgent = await actingAgentSlug(causingSessionId)
  const names = matchAutomations(list, { record: { kind, op }, byAgent }, false)
  const firedAt = new Date().toISOString()
  for (const name of names) await runAutomation(app, list.find((a) => a.name === name)!, { record, firedAt })
  return names.length
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** An `events` row (emitted inside a transaction): runs once the row is
 *  visible (committed); a rolled-back event runs nothing. */
export async function fireEventAutomations(event: Event, appIds: readonly string[], causingSessionId: string | null = null): Promise<number> {
  for (let attempt = 0; ; attempt++) {
    const [row] = await db.select({ id: events.id }).from(events).where(eq(events.id, event.id)).limit(1)
    if (row) break
    if (attempt >= 20) return 0
    await sleep(250)
  }
  let count = 0
  const firedAt = new Date().toISOString()
  // #VAPP-236: the agent loop guard: the emitting session, else the call's.
  const byAgent = await actingAgentSlug(event.sessionId ?? causingSessionId)
  for (const appId of appIds) {
    const app = await loadAutomationApp(appId)
    if (!app) continue
    const list = publishedAutomations(app.package)
    for (const name of matchAutomations(list, { event: event.name, byAgent }, false)) {
      await runAutomation(app, list.find((a) => a.name === name)!, { event: { ...(event.payload ?? {}), name: event.name }, firedAt })
      count += 1
    }
  }
  return count
}

const HOUR = 60 * 60 * 1000

/** One watchdog tick of every published schedule: the latest due minute
 *  since the last check (caught up once within `agentTrigger.catchUpHours`,
 *  never twice; a new automation starts from now), read in its `tz` or the
 *  team's time zone. Returns the runs started. */
export async function fireScheduledAutomations(now: Date = new Date()): Promise<number> {
  const catchUpSince = new Date(now.getTime() - contract.agentTrigger.catchUpHours * HOUR)
  const due = await db.transaction(async (tx) => {
    const rows = await tx
      .select({ ...APP_COLUMNS, teamZone: teams.timeZone })
      .from(apps)
      .innerJoin(teams, eq(teams.id, apps.teamId))
      .where(and(isNull(apps.deletedAt), sql`jsonb_path_exists(${apps.package}, '$.automations[*].on.schedule')`))
      .for(`update`, { of: apps, skipLocked: true })
    const out: { app: AutomationApp; automation: Automation; at: Date }[] = []
    for (const { teamZone, ...app } of rows) {
      const state = { ...(app.automationState ?? {}) }
      for (const automation of publishedAutomations(app.package)) {
        if (!(`schedule` in automation.on)) continue
        const fields = parseCron(automation.on.schedule.cron)
        if (!fields) continue
        const entry = { ...(state[automation.name] ?? {}) }
        const checked = entry.checkedAt ? new Date(entry.checkedAt) : null
        const after = checked && !Number.isNaN(checked.getTime()) ? new Date(Math.max(checked.getTime(), catchUpSince.getTime())) : new Date(now.getTime() - 60_000)
        const at = latestCronMatch(fields, after, now, automation.on.schedule.tz ?? teamZone)
        if (at) out.push({ app, automation, at })
        state[automation.name] = { ...entry, checkedAt: now.toISOString() }
      }
      await tx.update(apps).set({ automationState: state }).where(eq(apps.id, app.id))
    }
    return out
  })
  for (const { app, automation, at } of due) {
    await runAutomation(app, automation, { firedAt: at.toISOString() }).catch((err) => console.error(`[automations] schedule run failed:`, err))
  }
  return due.length
}

export interface AutomationRow {
  name: string
  on: Automation[`on`]
  tool: string
  paused: SponsorState | null
  /** #VAPP-243: the member a run acts as (name, else email); null when paused or unknown. */
  sponsorName: string | null
  lastRunAt: string | null
  lastStatus: CallStatus | null
  lastError: string | null
  lastFailureAt: string | null
  /** #VAPP-255: runs the rate cap dropped in the last 24 h (`rateLimited` audit rows). */
  droppedRuns: number
}

/** Settings › Automations: every automation of the published package with
 *  its live sponsor state and its newest audit rows. */
export async function automationRows(app: Pick<App, `id` | `teamId` | `package` | `publishedBy`>): Promise<AutomationRow[]> {
  const list = publishedAutomations(app.package)
  if (list.length === 0) return []
  const latest = await db
    .select({
      clientId: mcpCalls.clientId,
      lastRunAt: sql<Date>`max(${mcpCalls.createdAt})`,
      lastFailureAt: sql<Date | null>`max(${mcpCalls.createdAt}) FILTER (WHERE ${mcpCalls.status} <> 'ok')`,
      lastStatus: sql<string>`(array_agg(${mcpCalls.status} ORDER BY ${mcpCalls.createdAt} DESC))[1]`,
      lastError: sql<string | null>`(array_agg(${mcpCalls.error} ORDER BY ${mcpCalls.createdAt} DESC))[1]`,
      droppedRuns: sql<number>`(count(*) FILTER (WHERE ${mcpCalls.error} = 'rateLimited' AND ${mcpCalls.createdAt} > now() - interval '24 hours'))::int`,
    })
    .from(mcpCalls)
    .where(and(eq(mcpCalls.appId, app.id), like(mcpCalls.clientId, `automation:%`)))
    .groupBy(mcpCalls.clientId)
  const byClient = new Map(latest.map((row) => [row.clientId, row]))
  const iso = (d: Date | string | null | undefined) => (d ? new Date(d).toISOString() : null)
  const out: AutomationRow[] = []
  for (const automation of list) {
    const sponsor = await sponsorOf(app, automation)
    const row = byClient.get(automationClientId(automation.name))
    const [person] =
      `userId` in sponsor ? await db.select({ name: users.name, email: users.email }).from(users).where(eq(users.id, sponsor.userId)).limit(1) : []
    out.push({
      name: automation.name,
      on: automation.on,
      tool: automationTool(automation.run),
      paused: `paused` in sponsor ? sponsor.paused : null,
      sponsorName: person ? person.name || person.email : null,
      lastRunAt: iso(row?.lastRunAt),
      lastStatus: (row?.lastStatus as CallStatus | undefined) ?? null,
      lastError: row?.lastError ?? null,
      lastFailureAt: iso(row?.lastFailureAt),
      droppedRuns: row?.droppedRuns ?? 0,
    })
  }
  return out
}
