import { and, eq, sql } from "drizzle-orm"
import { MAX_AGENT_TRIGGERS, type AgentTrigger, type AgentTriggerState } from "@exp/db-schema/domain"
import { automationClientId, validateAutomations, type Automation } from "@exp/emcp/automations"
import { agentSlug } from "@exp/emcp/exposes"
import { db } from "@/db/connection"
import { agents, apps, appVersions, mcpCalls, teams, type App } from "@/db/schema"
import { automationAsTrigger, packageContext, triggerAutomationName } from "@/lib/agents/app-triggers"
import { createAppAgentInTx } from "@/lib/apps/app-agent"

// #EXP-1829 (ONE trigger system): the boot pass that moves every app's
// package `automations` onto its agent's triggers (`runBootstrapPass`,
// idempotent: an app whose packages hold no `automations` is never read).
// Per app, in one transaction:
//   - the PUBLISHED package's valid automations (the ones that ran) become
//     triggers on the app's agent (created when an older app has none, owned
//     by its publisher, else its creator), appended to its triggers;
//   - each keeps its schedule bookkeeping (`checkedAt`, so no tick fires twice
//     or is lost) and its audit history (`mcp_calls.client_id` renamed to the
//     trigger's `automation:t_<id>`);
//   - `automations` leaves `apps.package`, `apps.draft_package` and every
//     `app_versions.package` (a draft's unpublished automations never ran
//     and are dropped; a restore can never bring them back).

function automationsOf(pkg: unknown): unknown[] {
  const list = pkg && typeof pkg === `object` ? (pkg as { automations?: unknown }).automations : undefined
  return Array.isArray(list) ? list : []
}

const HAS = (column: unknown) => sql`(${column} -> 'automations') IS NOT NULL`

export async function migrateAppAutomations(): Promise<number> {
  const rows = await db
    .select({ id: apps.id })
    .from(apps)
    .where(
      sql`${HAS(apps.package)} OR ${HAS(apps.draftPackage)} OR EXISTS (SELECT 1 FROM ${appVersions} v WHERE v.app_id = ${apps.id} AND (v.package -> 'automations') IS NOT NULL)`
    )
  let moved = 0
  for (const { id } of rows) {
    try {
      moved += await migrateOne(id)
    } catch (err) {
      console.error(`[automations-migrate] app ${id} kept its automations:`, err)
    }
  }
  if (rows.length > 0) console.info(`[automations-migrate] ${rows.length} apps, ${moved} automations moved to their agents' triggers`)
  return moved
}

async function migrateOne(appId: string): Promise<number> {
  return db.transaction(async (tx) => {
    const [app] = await tx.select().from(apps).where(eq(apps.id, appId)).for(`update`).limit(1)
    if (!app) return 0
    let moved = 0
    const published = automationsOf(app.package)
    if (published.length > 0) {
      const errors = validateAutomations(published, packageContext(app.package))
      const broken = new Set(errors.map((e) => e.index))
      const list = broken.has(null) ? [] : (published.filter((_, i) => !broken.has(i)) as Automation[])
      if (list.length > 0) {
        let agentId = app.agentId
        if (!agentId) {
          const owner = app.publishedBy ?? app.createdBy
          if (!owner) throw new Error(`no agent and no owner to create one`)
          agentId = await createAppAgentInTx(tx, app as App, owner)
        }
        const [agent] = await tx.select().from(agents).where(eq(agents.id, agentId)).for(`update`).limit(1)
        if (!agent) throw new Error(`agent ${agentId} is gone`)
        const [team] = await tx.select({ timeZone: teams.timeZone }).from(teams).where(eq(teams.id, app.teamId)).limit(1)
        const state: AgentTriggerState = { ...(agent.triggerState ?? {}) }
        const added: AgentTrigger[] = []
        const room = Math.max(0, MAX_AGENT_TRIGGERS - (agent.triggers ?? []).length)
        for (const automation of list) {
          if (added.length >= room) {
            console.warn(`[automations-migrate] app ${appId}: ${automation.name} dropped (the agent holds ${MAX_AGENT_TRIGGERS} triggers)`)
            continue
          }
          const trigger = automationAsTrigger(automation, { teamZone: team?.timeZone ?? null, ownSlug: agentSlug(agent.name) })
          if (!trigger) continue
          added.push(trigger)
          const before = app.automationState?.[automation.name]
          if (before?.checkedAt) state[trigger.id] = { checkedAt: before.checkedAt }
          await tx
            .update(mcpCalls)
            .set({ clientId: automationClientId(triggerAutomationName(trigger.id)) })
            .where(and(eq(mcpCalls.appId, app.id), eq(mcpCalls.clientId, automationClientId(automation.name))))
        }
        await tx
          .update(agents)
          .set({ triggers: [...(agent.triggers ?? []), ...added], triggerState: state })
          .where(eq(agents.id, agent.id))
        moved = added.length
      }
    }
    await tx
      .update(apps)
      .set({
        package: sql`CASE WHEN ${HAS(apps.package)} THEN ${apps.package} - 'automations' ELSE ${apps.package} END`,
        draftPackage: sql`CASE WHEN ${HAS(apps.draftPackage)} THEN ${apps.draftPackage} - 'automations' ELSE ${apps.draftPackage} END`,
        automationState: {},
      })
      .where(eq(apps.id, app.id))
    await tx
      .update(appVersions)
      .set({ package: sql`${appVersions.package} - 'automations'` })
      .where(and(eq(appVersions.appId, app.id), HAS(appVersions.package)))
    return moved
  })
}
