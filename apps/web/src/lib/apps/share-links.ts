// VAPP-241 (eApps P1, fixtures/app-share.json, docs/spec/apps.md §9.4): an
// app's PUBLIC LINKS. An owner turns a link of the PUBLISHED package's `share`
// on; a guest reaches `/s/<token>`: `submit` creates one record as the link's
// sponsor, `view` reads a server snapshot of the named fields. A guest of
// anything but a live link gets one answer, the same as an unknown token.
import { createHmac, randomBytes } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { and, count, eq, gt, lt } from "drizzle-orm"
import { APP_SHARE_CAPS, APP_SHARE_COPY, linkState, shareFields, underShareCaps, type ShareLinkState, type ShareMode, type ShareSchema } from "@exp/domain-contract/app-share"
import { CORE_CATALOG_ID, PACKAGE_FORMAT_VERSION, type AppPackage, type PackageShare, type RecordKind } from "@exponential_app/eui"
import { db } from "@/db/connection"
import { apps, appShareLinks, appShareSubmissions, teamMembers } from "@/db/schema"
import { readAppRecordRows, serverRecordsValue, teamMemberNames, teamToday, upsertRecord } from "@/lib/apps/records-service"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"

const HOUR_MS = 3_600_000
const DAY_MS = 86_400_000

export interface ShareRow {
  name: string
  mode: ShareMode
  kind: string
  state: ShareLinkState
  /** The guest URL while the row has a live token, for the owner's Copy link. */
  url: string | null
  expiresAt: string | null
}

const sharesOf = (pkg: unknown): PackageShare[] => {
  const share = (pkg as { share?: unknown } | null)?.share
  return Array.isArray(share) ? (share as PackageShare[]) : []
}

const shareUrl = (token: string) => `${appBaseUrl()}/s/${token}`
const newToken = () => randomBytes(32).toString(`base64url`)

/** Settings › Public links: every share of the published package, then the
 *  links it no longer declares that are still on. */
export async function shareRows(app: { id: string; package: unknown }, now = new Date()): Promise<ShareRow[]> {
  const shares = sharesOf(app.package)
  const links = await db.select().from(appShareLinks).where(eq(appShareLinks.appId, app.id))
  const row = (name: string, mode: ShareMode, kind: string, declared: boolean): ShareRow => {
    const link = links.find((l) => l.name === name) ?? null
    const state = linkState(link && { revokedAt: link.revokedAt?.toISOString() ?? null, expiresAt: link.expiresAt?.toISOString() ?? null }, declared, now.toISOString())
    return { name, mode, kind, state, url: link && state !== `off` ? shareUrl(link.token) : null, expiresAt: link?.expiresAt?.toISOString().slice(0, 10) ?? null }
  }
  const declared = shares.map((s) => row(s.name, s.mode, s.kind, true))
  const orphans = links.filter((l) => !l.revokedAt && !shares.some((s) => s.name === l.name)).map((l) => row(l.name, `submit`, ``, false))
  return [...declared, ...orphans]
}

function declaredShare(app: { package: unknown }, name: string): PackageShare {
  const share = sharesOf(app.package).find((s) => s.name === name)
  if (!share) throw new TRPCError({ code: `NOT_FOUND`, message: `The published app has no public link ${name}` })
  return share
}

/** Turn a link on: a new token, the sponsor = this owner, the package's
 *  expiry from now. */
export async function enableShareLink(userId: string, app: { id: string; teamId: string; package: unknown }, name: string, now = new Date()): Promise<void> {
  const share = declaredShare(app, name)
  const expiresAt = share.expires ? new Date(now.getTime() + share.expires * DAY_MS) : null
  await db
    .insert(appShareLinks)
    .values({ appId: app.id, teamId: app.teamId, name, token: newToken(), enabledBy: userId, expiresAt })
    .onConflictDoUpdate({ target: [appShareLinks.appId, appShareLinks.name], set: { token: newToken(), enabledBy: userId, expiresAt, revokedAt: null, rotatedAt: null, updatedAt: now } })
}

/** A new token on the same link: the old URL stops at once, the expiry and
 *  the sponsor stay. */
export async function rotateShareLink(app: { id: string }, name: string, now = new Date()): Promise<void> {
  const rows = await db
    .update(appShareLinks)
    .set({ token: newToken(), rotatedAt: now, updatedAt: now })
    .where(and(eq(appShareLinks.appId, app.id), eq(appShareLinks.name, name)))
    .returning({ id: appShareLinks.id })
  if (rows.length === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `This link is off` })
}

export async function revokeShareLink(app: { id: string }, name: string, now = new Date()): Promise<void> {
  await db.update(appShareLinks).set({ revokedAt: now, updatedAt: now }).where(and(eq(appShareLinks.appId, app.id), eq(appShareLinks.name, name)))
}

interface LiveLink {
  link: typeof appShareLinks.$inferSelect
  sponsor: string
  app: { id: string; teamId: string; name: string; package: unknown }
  share: PackageShare
  kind: RecordKind
  fields: string[]
}

/** The live link of a token, else null (unknown, off, expired, orphaned, a
 *  deleted app, a sponsor who left or is no longer an owner). */
async function liveLink(token: string, now = new Date()): Promise<LiveLink | null> {
  if (!token || token.length > 64) return null
  const [row] = await db.select({ link: appShareLinks, app: { id: apps.id, teamId: apps.teamId, name: apps.name, package: apps.package, deletedAt: apps.deletedAt } }).from(appShareLinks).innerJoin(apps, eq(apps.id, appShareLinks.appId)).where(eq(appShareLinks.token, token)).limit(1)
  if (!row || row.app.deletedAt) return null
  const share = sharesOf(row.app.package).find((s) => s.name === row.link.name)
  const state = linkState({ revokedAt: row.link.revokedAt?.toISOString() ?? null, expiresAt: row.link.expiresAt?.toISOString() ?? null }, !!share, now.toISOString())
  const sponsor = row.link.enabledBy
  if (state !== `on` || !share || !sponsor) return null
  const [member] = await db.select({ role: teamMembers.role }).from(teamMembers).where(and(eq(teamMembers.teamId, row.app.teamId), eq(teamMembers.userId, sponsor))).limit(1)
  if (member?.role !== `owner`) return null
  const kind = (row.app.package as { records?: Record<string, RecordKind> }).records?.[share.kind]
  if (!kind) return null
  return { link: row.link, sponsor, app: row.app, share, kind, fields: shareFields(kind.schema as ShareSchema, share.fields) }
}

/** The kind as a guest sees it: only the named fields, no refs, policy,
 *  computed or transitions. */
function guestKind(kind: RecordKind, fields: readonly string[]): RecordKind {
  const properties = Object.fromEntries(fields.map((f) => [f, kind.schema.properties[f]!]))
  return { title: kind.title, schema: { ...kind.schema, properties, required: (kind.schema.required ?? []).filter((f) => fields.includes(f)) } } as RecordKind
}

export interface GuestPage {
  mode: ShareMode
  title: string
  package: AppPackage
  /** A view link's snapshot, at the RecordTable's `records:` binding. */
  value?: { rows: Record<string, unknown>[]; count: number }
}

/** What a guest's page renders: a one-template package with nothing but
 *  the one kind's named fields, and a view link's snapshot. Null = gone. */
export async function guestPage(token: string): Promise<GuestPage | null> {
  const live = await liveLink(token)
  if (!live) return null
  const { share, fields, app } = live
  const node = share.mode === `submit` ? { id: `form`, component: `RecordForm`, kind: share.kind } : { id: `table`, component: `RecordTable`, kind: share.kind, editable: false, add: false }
  const title = live.kind.title ?? app.name
  const pkg = {
    formatVersion: PACKAGE_FORMAT_VERSION,
    id: `share.${share.name}`,
    name: title,
    version: `1.0.0`,
    catalogId: CORE_CATALOG_ID,
    functions: share.mode === `submit` ? [`records.upsert`] : [],
    sources: [`records:${share.kind}`],
    records: { [share.kind]: guestKind(live.kind, fields) },
    templates: { main: { components: [{ id: `root`, component: `Page`, title, children: [node.id] }, node] } },
  } as unknown as AppPackage
  if (share.mode === `submit`) return { mode: share.mode, title, package: pkg }
  return { mode: share.mode, title, package: pkg, value: await snapshot(live) }
}

/** The view link's frozen query, narrowed: `id`, `updatedAt` and the named
 *  fields, at most `caps.viewRows` rows, never an author. */
async function snapshot(live: LiveLink): Promise<{ rows: Record<string, unknown>[]; count: number }> {
  const { app, share, sponsor, fields } = live
  const query = share.query ? (((app.package as { queries?: Record<string, Record<string, unknown>> }).queries ?? {})[share.query] ?? {}) : {}
  const { rows } = await readAppRecordRows(app, share.kind, {}, sponsor)
  const [names, today] = await Promise.all([teamMemberNames(app.teamId), teamToday(app.teamId)])
  const { from: _from, ...json } = query
  const out = serverRecordsValue(rows, share.kind, json, { me: sponsor, names, today })
  const shown = ((out.value?.rows ?? []) as Record<string, unknown>[]).slice(0, APP_SHARE_CAPS.viewRows)
  return { rows: shown.map((row) => Object.fromEntries([`id`, `updatedAt`, ...fields].filter((k) => row[k] !== undefined).map((k) => [k, row[k]]))), count: shown.length }
}

/** The IP as the audit keeps it: a keyed hash, never the address. */
const ipHash = (request: Request) => createHmac(`sha256`, process.env.BETTER_AUTH_SECRET ?? `share`).update(clientIpFromRequest(request)).digest(`hex`)

const json = (status: number, body: Record<string, unknown>) => new Response(JSON.stringify(body), { status, headers: { "content-type": `application/json` } })

async function audit(link: LiveLink[`link`], hash: string, outcome: typeof appShareSubmissions.$inferInsert.outcome, recordId?: string) {
  await db.insert(appShareSubmissions).values({ linkId: link.id, appId: link.appId, teamId: link.teamId, ipHash: hash, outcome, recordId: recordId ?? null })
}

/** `POST /api/share/<token>`: one guest submission (app-share.json). */
export async function handleShareSubmit(request: Request, token: string, now = new Date()): Promise<Response> {
  const live = await liveLink(token, now)
  if (!live || live.share.mode !== `submit`) return json(404, { error: APP_SHARE_COPY.gone })
  const text = await request.text()
  if (new TextEncoder().encode(text).length > APP_SHARE_CAPS.maxBodyBytes) return json(413, { error: APP_SHARE_COPY.tooMany })
  let body: { data?: unknown; website?: unknown }
  try {
    body = JSON.parse(text) as typeof body
  } catch {
    return json(400, { error: `The form could not be read` })
  }
  const hash = ipHash(request)
  const link = live.link
  // The honeypot: answered like a success, nothing stored.
  if (typeof body.website === `string` && body.website) {
    await audit(link, hash, `honeypot`)
    return json(200, { ok: true })
  }
  const since = new Date(now.getTime() - HOUR_MS)
  const [perLink] = await db.select({ n: count() }).from(appShareSubmissions).where(and(eq(appShareSubmissions.linkId, link.id), gt(appShareSubmissions.createdAt, since)))
  const [perIp] = await db.select({ n: count() }).from(appShareSubmissions).where(and(eq(appShareSubmissions.linkId, link.id), eq(appShareSubmissions.ipHash, hash), gt(appShareSubmissions.createdAt, since)))
  if (!underShareCaps(perIp?.n ?? 0, perLink?.n ?? 0)) {
    await audit(link, hash, `rate_limited`)
    return json(429, { error: APP_SHARE_COPY.tooMany })
  }
  const sent = typeof body.data === `object` && body.data !== null ? (body.data as Record<string, unknown>) : {}
  const data = Object.fromEntries(live.fields.filter((f) => sent[f] !== undefined).map((f) => [f, sent[f]]))
  try {
    const { id } = await upsertRecord(live.sponsor, { id: live.app.id }, { kind: live.share.kind, data, via: `share:${live.share.name}`, shareName: live.share.name })
    await audit(link, hash, `created`, id)
    await db.delete(appShareSubmissions).where(and(eq(appShareSubmissions.linkId, link.id), lt(appShareSubmissions.createdAt, new Date(now.getTime() - APP_SHARE_CAPS.auditDays * DAY_MS))))
    return json(200, { ok: true })
  } catch (error) {
    if (error instanceof TRPCError && error.code === `BAD_REQUEST`) {
      await audit(link, hash, `invalid`)
      return json(400, { error: error.message })
    }
    if (error instanceof TRPCError) {
      await audit(link, hash, `quota`)
      return json(403, { error: APP_SHARE_COPY.closed })
    }
    throw error
  }
}
