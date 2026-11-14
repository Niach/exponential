// #VAPP-237 (eApps A3, docs/spec/apps.md §7.2, §12.3): an app's INBOUND
// WEBHOOKS. `POST /api/apps/hooks/<appId>/<name>` fires the app agent's
// webhook triggers on that name (#EXP-1829: a wake, or a run through A1's
// executor: the owner, the app cap, the audit) once the delivery verifies
// against the hook's
// vault-sealed secret (@exp/emcp automations.json `webhook`: the HMAC
// header, the timestamp window, the nonce cache, ≤ 64 KB JSON). An owner
// mints (and rotates) a hook's secret on the app's Settings; it is shown
// once.
import { TRPCError } from "@trpc/server"
import { and, eq, gt, lt, sql } from "drizzle-orm"
import { automationClientId } from "@exp/emcp/automations"
import type { AgentTrigger } from "@exp/db-schema/domain"
import { newWebhookSecret, verifyWebhook, WEBHOOK, webhookEvent, webhookPath, webhookStatus } from "@exp/emcp/webhooks"
import { db } from "@/db/connection"
import { appHookNonces, appHooks, mcpCalls, teamMembers } from "@/db/schema"
import { auditFiring, loadAutomationApp, runAutomation, wakeAppAgent, type AutomationApp } from "@/lib/apps/automations"
import { runnableTriggers, triggerAutomationName, wakeTriggers } from "@/lib/agents/app-triggers"
import { openSecret, sealSecret } from "@/lib/emcp/vault"
import { hookSlot } from "@/lib/apps/hook-slot"
import { appBaseUrl } from "@/lib/notification-email-policy"

export { hookSlot }

/** The hook names the app agent's enabled webhook triggers listen on, in order. */
export function hookNames(triggers: readonly AgentTrigger[] | null | undefined): string[] {
  const names: string[] = []
  for (const trigger of triggers ?? []) {
    if (trigger?.kind === `webhook` && trigger.enabled !== false && !names.includes(trigger.name)) names.push(trigger.name)
  }
  return names
}

/** The hook names of an app (its agent's webhook triggers). */
async function appHookNames(appId: string): Promise<string[]> {
  const app = await loadAutomationApp(appId)
  return app ? hookNames(app.triggers) : []
}

export interface HookRow {
  name: string
  url: string
  /** A secret was minted (deliveries verify). */
  hasSecret: boolean
  rotatedAt: string | null
}

/** Settings › Automations: every hook of the app agent's triggers. */
export async function hookRows(app: { id: string }): Promise<HookRow[]> {
  const names = await appHookNames(app.id)
  if (names.length === 0) return []
  const rows = await db.select({ name: appHooks.name, updatedAt: appHooks.updatedAt }).from(appHooks).where(eq(appHooks.appId, app.id))
  const byName = new Map(rows.map((r) => [r.name, r]))
  return names.map((name) => {
    const row = byName.get(name)
    return { name, url: `${appBaseUrl()}${webhookPath(app.id, name)}`, hasSecret: !!row, rotatedAt: row ? row.updatedAt.toISOString() : null }
  })
}

/** Mints (or rotates) a hook's secret: owners only, a hook the app agent's
 *  triggers listen on. Returns the secret ONCE. */
export async function rotateHookSecret(userId: string, app: { id: string; teamId: string }, name: string): Promise<{ name: string; url: string; secret: string }> {
  const [member] = await db
    .select({ role: teamMembers.role })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, app.teamId), eq(teamMembers.userId, userId)))
    .limit(1)
  if (member?.role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner mints a webhook secret` })
  if (!(await appHookNames(app.id)).includes(name)) throw new TRPCError({ code: `NOT_FOUND`, message: `The app has no webhook ${JSON.stringify(name)}` })
  const secret = newWebhookSecret()
  await db.transaction(async (tx) => {
    const [existing] = await tx.select({ id: appHooks.id }).from(appHooks).where(and(eq(appHooks.appId, app.id), eq(appHooks.name, name))).for(`update`).limit(1)
    const id = existing?.id ?? crypto.randomUUID()
    const secretCiphertext = sealSecret(secret, hookSlot(id))
    if (existing) {
      await tx.update(appHooks).set({ secretCiphertext, rotatedBy: userId, updatedAt: new Date() }).where(eq(appHooks.id, id))
      // A rotation starts a fresh replay cache.
      await tx.delete(appHookNonces).where(eq(appHookNonces.hookId, id))
    } else {
      await tx.insert(appHooks).values({ id, appId: app.id, teamId: app.teamId, name, secretCiphertext, rotatedBy: userId })
    }
  })
  return { name, url: `${appBaseUrl()}${webhookPath(app.id, name)}`, secret }
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

const json = (status: number, body: Record<string, unknown>) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": `application/json` } })
const refuse = (error: Parameters<typeof webhookStatus>[0]) => json(webhookStatus(error), { ok: false, error })

/** Deliveries of the hook in the last minute (its first automation's rows:
 *  every delivery under the cap leaves exactly one there). */
async function deliveriesLastMinute(appId: string, clientId: string, now: Date): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(mcpCalls)
    .where(and(eq(mcpCalls.appId, appId), eq(mcpCalls.clientId, clientId), gt(mcpCalls.createdAt, new Date(now.getTime() - 60_000))))
  return row?.n ?? 0
}

const replayWindowMs = () => 2 * WEBHOOK.toleranceSeconds * 1000

/** One delivery. */
export async function handleWebhookDelivery(request: Request, appId: string, name: string, now: Date = new Date()): Promise<Response> {
  if (!UUID.test(appId)) return refuse(`unknownHook`)
  const app: AutomationApp | null = await loadAutomationApp(appId)
  if (!app) return refuse(`unknownHook`)
  const listening = (t: AgentTrigger) => t.kind === `webhook` && t.name === name
  const runs = runnableTriggers(app.triggers, app.package).filter((r) => listening(r.trigger))
  const wakes = wakeTriggers(app.triggers).filter(listening)
  if (runs.length === 0 && wakes.length === 0) return refuse(`unknownHook`)
  const [hook] = await db.select().from(appHooks).where(and(eq(appHooks.appId, app.id), eq(appHooks.name, name))).limit(1)
  const secret = hook ? openSecret(hook.secretCiphertext, hookSlot(hook.id)) : null
  if (!hook || !secret) return refuse(`unknownHook`)
  // The hook's first trigger carries its rows: every delivery under the cap leaves exactly one there.
  const first = runs[0]?.trigger ?? wakes[0]!
  const firstName = triggerAutomationName(first.id)
  if ((await deliveriesLastMinute(app.id, automationClientId(firstName), now)) >= WEBHOOK.callsPerMinute) return refuse(`rateLimited`)

  const startedAt = Date.now()
  const auditRefusal = (error: string) => auditFiring(app, firstName, `webhook:${name}`, app.ownerId, `denied`, error, startedAt, { hook: name })
  const declared = Number(request.headers.get(`content-length`) ?? `0`)
  let body = ``
  let bytes = declared
  if (!(declared > WEBHOOK.bodyMax)) {
    const raw = new Uint8Array(await request.arrayBuffer())
    bytes = raw.byteLength
    if (bytes <= WEBHOOK.bodyMax) body = new TextDecoder().decode(raw)
  }
  const seen = (
    await db
      .select({ nonce: appHookNonces.nonce })
      .from(appHookNonces)
      .where(and(eq(appHookNonces.hookId, hook.id), gt(appHookNonces.createdAt, new Date(now.getTime() - replayWindowMs()))))
  ).map((r) => r.nonce)
  const verdict = verifyWebhook({ header: request.headers.get(WEBHOOK.header), body, bytes, secret, now: Math.floor(now.getTime() / 1000), seen })
  if (!verdict.ok) {
    await auditRefusal(verdict.error)
    return refuse(verdict.error)
  }
  // The nonce lands first: a concurrent copy of the same delivery loses.
  const [kept] = await db.insert(appHookNonces).values({ hookId: hook.id, nonce: verdict.nonce, createdAt: now }).onConflictDoNothing().returning({ nonce: appHookNonces.nonce })
  if (!kept) {
    await auditRefusal(`replayed`)
    return refuse(`replayed`)
  }
  const firedAt = now.toISOString()
  const input = { event: webhookEvent(name, verdict.body), firedAt }
  const fired: { name: string; status: string }[] = []
  for (const entry of runs) fired.push({ name: entry.automation.name, status: await runAutomation(app, entry, input) })
  for (const trigger of wakes) fired.push({ name: triggerAutomationName(trigger.id), status: await wakeAppAgent(app, trigger, input) })
  return json(webhookStatus(`accepted`), { ok: true, runs: fired })
}

/** Nonces past the replay window (the watchdog). */
export async function pruneHookNonces(now: Date = new Date()): Promise<number> {
  const gone = await db
    .delete(appHookNonces)
    .where(lt(appHookNonces.createdAt, new Date(now.getTime() - replayWindowMs())))
    .returning({ nonce: appHookNonces.nonce })
  return gone.length
}
