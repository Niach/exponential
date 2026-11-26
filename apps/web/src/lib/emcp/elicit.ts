// #NEW-147 (packages/emcp fixtures/elicitation.json, D6 of #NEW-143):
// elicitation on `/api/mcp` for EXTERNAL hosts that declared it, over MRTR
// (mrtr.ts `askInput` / `redeemInput`). Before a call runs: (1) the CONFIRM
// of every destructive op (deletes.json) and of a session stop, (2) the
// TEAM pick of a new root that names no team, (3) the DEVICE pick of a new
// root that names no device. After it ran: a failure a person fixes on a
// page (billing checkout on cloud, the GitHub App) becomes a URL-mode ask
// to `${APP_URL}/mcp/elicit/<id>` (routes/mcp/elicit/$id.ts). A client
// without the mode gets today's behaviour (`legacy`), never an error. Our
// own sessions never get asked (they keep today's behaviour).
import { randomBytes } from "node:crypto"
import { ProtocolError } from "@modelcontextprotocol/server"
import { and, eq, inArray } from "drizzle-orm"
import {
  CONFIRM_KEY,
  confirmGate,
  confirmOutcome,
  ELICIT_RESULTS,
  pickNeeded,
  pickOutcome,
  pickRequest,
  urlRequest,
  type ElicitationModes,
  type PickChoice,
  type PickKind,
  type UrlPurpose,
} from "@exp/emcp/elicitation"
import { MRTR, PROTOCOL_ERRORS } from "@exp/emcp/protocol"
import fixture from "@exp/emcp/fixtures/elicitation.json" with { type: "json" }
import { db } from "@/db/connection"
import { mcpElicitations, teams } from "@/db/schema"
import { getPersonalTeamId, getUserTeamIds } from "@/lib/auth/membership"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "@/lib/plan-limit-error"
import { composeDeviceList } from "@/lib/steer-devices"
import { visibleDeviceRows } from "@/lib/trpc/devices"
import { githubAppInstallUrl } from "@/lib/integrations/github-app"
import { askInput, escapeAsRpcError, redeemInput, type InputRequiredResult } from "./mrtr"
import { isTeamFullyGranted, type McpAccess } from "./scope"
import type { EmcpToolContext } from "./request-context"

type ToolResult = { content: unknown[]; isError?: boolean; structuredContent?: unknown }

/** What the asks of one call carry from round to round (the sealed `data`). */
interface ElicitData {
  confirmed?: boolean
  teamId?: string
  deviceId?: string
  /** The pick a round waits on, and the consts it offered. */
  awaiting?: PickKind
  offered?: string[]
  /** A URL flow's row (the same id on every retry). */
  elicitationId?: string
}

export type ElicitGate = { kind: `go`; args: Record<string, unknown>; data: ElicitData } | { kind: `answer`; result: ToolResult | InputRequiredResult }

const refused = (text: string): ToolResult => ({ content: [{ type: `text`, text }], isError: true })

const PICKS: Record<PickKind, { key: string; property: string }> = {
  team: { key: fixture.teamPick.key, property: fixture.teamPick.property },
  device: { key: fixture.devicePick.key, property: fixture.devicePick.property },
}

/** The teams a new root may start in: the member's teams its grant reaches whole, by name. */
export async function teamChoices(userId: string, access: McpAccess): Promise<PickChoice[]> {
  const ids = (await getUserTeamIds(userId)).filter((id) => isTeamFullyGranted(access, id))
  if (ids.length === 0) return []
  const rows = await db.select({ id: teams.id, name: teams.name }).from(teams).where(and(inArray(teams.id, ids)))
  return rows.map((row) => ({ id: row.id, name: row.name }))
}

/** The ONLINE devices the member may start a session of `teamId` on. */
export async function deviceChoices(userId: string, teamId: string | undefined): Promise<PickChoice[]> {
  const visible = await visibleDeviceRows(db, userId, teamId)
  const list = composeDeviceList(visible.rows, visible.ownerNames, new Date(), userId, teamId)
  return list.filter((device) => device.online !== false).map((device) => ({ id: device.deviceId, name: device.deviceLabel }))
}

function rawResponse(ctx: EmcpToolContext, key: string): unknown {
  return (ctx.mcpReq?.inputResponses ?? {})[key]
}

const invalidAnswer = () => new ProtocolError(PROTOCOL_ERRORS.invalidParams, `inputResponses: the answer is not one that was offered.`)

/**
 * The asks in front of one call of `tool` with `args` (`{op, ...}` for a
 * noun). `go` = run it with these arguments (a pick filled in); `answer` =
 * return this instead (an `input_required`, or the declined / cancelled /
 * nothing-picked result).
 */
export async function elicitBeforeCall(
  ctx: EmcpToolContext,
  spec: { tool: string; args: Record<string, unknown>; userId: string; access: McpAccess; external: boolean }
): Promise<ElicitGate> {
  const args = { ...spec.args }
  const redeemed = redeemInput<ElicitData>(ctx)
  const data: ElicitData = { ...(redeemed?.data ?? {}) }
  if (!spec.external || ctx.client.era !== `modern`) return { kind: `go`, args, data }
  const modes: ElicitationModes = ctx.client.elicitation

  // (1) The confirm (form): a destructive op with one id, or a stop.
  const confirm = confirmGate(spec.tool, args, modes)
  if (confirm.kind === `confirm` && !data.confirmed) {
    const outcome = redeemed && data.awaiting === undefined ? confirmOutcome(rawResponse(ctx, CONFIRM_KEY)) : `reask`
    if (outcome === `invalid`) return { kind: `answer`, result: escapeAsRpcError(ctx, invalidAnswer()) }
    if (outcome === `declined`) return { kind: `answer`, result: refused(ELICIT_RESULTS.declined) }
    if (outcome === `cancelled`) return { kind: `answer`, result: refused(ELICIT_RESULTS.cancelled) }
    if (outcome === `run`) data.confirmed = true
    else {
      const ask = askInput(ctx, { inputRequests: { [CONFIRM_KEY]: confirm.request }, data: { ...data, awaiting: undefined } })
      if (ask) return { kind: `answer`, result: ask }
    }
  }

  // (2) + (3) The picks (form), the team first: a device is the team's.
  for (const pick of [`team`, `device`] as const) {
    if (!pickNeeded(pick, spec.tool, args)) continue
    const { key, property } = PICKS[pick]
    const chosen = pick === `team` ? data.teamId : data.deviceId
    if (chosen) {
      args[property] = chosen
      continue
    }
    if (redeemed && data.awaiting === pick) {
      const outcome = pickOutcome(property, data.offered ?? [], rawResponse(ctx, key))
      if (outcome.kind === `invalid`) return { kind: `answer`, result: escapeAsRpcError(ctx, invalidAnswer()) }
      if (outcome.kind === `noPick`) return { kind: `answer`, result: refused(ELICIT_RESULTS.noPick) }
      if (outcome.kind === `use`) {
        args[property] = outcome.id
        if (pick === `team`) data.teamId = outcome.id
        else data.deviceId = outcome.id
        data.awaiting = undefined
        continue
      }
    }
    const choices =
      pick === `team`
        ? await teamChoices(spec.userId, spec.access)
        : await deviceChoices(spec.userId, typeof args.teamId === `string` ? args.teamId : undefined)
    const verdict = pickRequest(pick, choices, modes)
    if (verdict.kind === `use`) {
      args[property] = verdict.id
      if (pick === `team`) data.teamId = verdict.id
      else data.deviceId = verdict.id
      continue
    }
    if (verdict.kind === `legacy`) continue
    const ask = askInput(ctx, {
      inputRequests: { [key]: verdict.request },
      data: { ...data, awaiting: pick, offered: choices.map((choice) => choice.id) },
    })
    if (ask) return { kind: `answer`, result: ask }
  }
  return { kind: `go`, args, data }
}

// ── URL mode ──────────────────────────────────────────────────────────

const GITHUB_APP_FAILURE = /GitHub App (isn't installed|has no access)/

function errorText(result: ToolResult): string {
  const first = result.content.find((c) => !!c && typeof c === `object` && (c as { type?: unknown }).type === `text`) as { text?: unknown } | undefined
  return typeof first?.text === `string` ? first.text : ``
}

/** The page a failed call's person fixes it on, or null. Billing is the
 *  team's (`teamId` = the call's team, else the member's personal one). */
export async function urlPurposeOf(
  result: ToolResult,
  userId: string,
  teamId: string | null
): Promise<{ purpose: UrlPurpose; target: string } | null> {
  if (!result.isError) return null
  const text = errorText(result)
  if (text.includes(PLAN_LIMIT_MESSAGE_PREFIX) && process.env.CLOUD_INSTANCE === `true`) {
    const id = teamId ?? (await getPersonalTeamId(userId))
    const [team] = id ? await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, id)).limit(1) : []
    return team ? { purpose: `billing_checkout`, target: `/t/${team.slug}/settings/billing` } : null
  }
  const install = GITHUB_APP_FAILURE.test(text) ? githubAppInstallUrl() : null
  if (install) return { purpose: `github_app`, target: install }
  return null
}

/** An opaque URL-elicitation id (32 random bytes, base64url: `url.idPattern`). */
export const newElicitationId = () => randomBytes(32).toString(`base64url`)

/**
 * After a call failed on something a person fixes on a page: the URL-mode
 * `input_required` naming `${APP_URL}/mcp/elicit/<id>` (a row bound to the
 * member, the purpose and where it leads), or null = today's result. A
 * retry still failing asks again with the same id.
 */
export async function urlElicitAfterCall(
  ctx: EmcpToolContext,
  result: ToolResult,
  spec: { userId: string; external: boolean; data: ElicitData; teamId?: string | null; subject?: string | null }
): Promise<InputRequiredResult | null> {
  if (!spec.external || ctx.client.era !== `modern` || !ctx.client.elicitation.url) return null
  const found = await urlPurposeOf(result, spec.userId, spec.teamId ?? null)
  if (!found) return null
  return askUrl(ctx, found.purpose, spec.subject ?? null, found.target, spec)
}

/** The URL-mode ask of one purpose (a row the page resolves), or null (no url mode). */
export async function askUrl(
  ctx: EmcpToolContext,
  purpose: UrlPurpose,
  subject: string | null,
  target: string,
  spec: { userId: string; data: ElicitData }
): Promise<InputRequiredResult | null> {
  const id = spec.data.elicitationId ?? newElicitationId()
  const verdict = urlRequest(purpose, subject, appBaseUrl(), id, ctx.client.elicitation)
  if (verdict.kind !== `ask`) return null
  const expiresAt = new Date(Date.now() + MRTR.ttlSeconds * 1000)
  await db
    .insert(mcpElicitations)
    .values({ id, purpose, userId: spec.userId, grant: ctx.principal.grant, target, expiresAt })
    .onConflictDoUpdate({ target: mcpElicitations.id, set: { expiresAt } })
  return askInput(ctx, { inputRequests: { [purpose]: verdict.request }, data: { ...spec.data, elicitationId: id } })
}
