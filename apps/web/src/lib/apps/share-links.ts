// VAPP-241 (eApps P1, fixtures/app-share.json, docs/spec/apps.md §9.4): an
// app's PUBLIC LINKS. An owner turns a link of the PUBLISHED package's `share`
// on; a guest reaches `/s/<token>`: `submit` creates one record as the link's
// sponsor, `view` reads a server snapshot of the named fields. A guest of
// anything but a live link gets one answer, the same as an unknown token.
// Locked by share-links.test.ts + share-links.real-db.test.ts.
import { createHmac, randomBytes } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { and, count, eq, gt, lt, isNull, ne, or, sql } from "drizzle-orm"
import { APP_SHARE_CAPS, APP_SHARE_COPY, linkState, shareFields, underShareCaps, type ShareLinkState, type ShareMode, type ShareSchema } from "@exp/domain-contract/app-share"
import { CORE_CATALOG_ID, guestTemplate, PACKAGE_FORMAT_VERSION, validatePackage, validatePackageShares, type AppPackage, type PackageShare, type RecordKind } from "@exponential_app/eui"
import { db } from "@/db/connection"
import { apps, appShareLinks, appShareSubmissions, teamMembers } from "@/db/schema"
import { readAppRecordRows, serverRecordsValue, teamMemberNames, teamToday, upsertRecord } from "@/lib/apps/records-service"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { clientIpFromRequest, TokenBucketLimiter } from "@/lib/widget/rate-limit"

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
const TOKEN_SHAPE = /^[A-Za-z0-9_-]{1,64}$/

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
    .where(and(eq(appShareLinks.appId, app.id), eq(appShareLinks.name, name), isNull(appShareLinks.revokedAt)))
    .returning({ id: appShareLinks.id })
  if (rows.length === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `This link is off` })
}

/** VAPP-247: a live link's expiry, `days` from now, null = never. The token and
 *  the sponsor stay; a revoked or expired link is off (enabling mints a new one). */
export async function expireShareLink(app: { id: string }, name: string, days: number | null, now = new Date()): Promise<void> {
  const expiresAt = days === null ? null : new Date(now.getTime() + days * DAY_MS)
  const rows = await db
    .update(appShareLinks)
    .set({ expiresAt, updatedAt: now })
    .where(and(eq(appShareLinks.appId, app.id), eq(appShareLinks.name, name), isNull(appShareLinks.revokedAt), or(isNull(appShareLinks.expiresAt), gt(appShareLinks.expiresAt, now))))
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
  if (!TOKEN_SHAPE.test(token)) return null
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
  return { link: row.link, sponsor, app: row.app, share, kind, fields: guestFields(kind, share.fields) }
}

/** The fields a guest may touch: `shareFields` without the kind's refs (a
 *  guest has no picker, and a posted ref would probe members and issues). */
function guestFields(kind: RecordKind, fields?: readonly string[]): string[] {
  const refs = kind.refs ?? {}
  const properties = Object.fromEntries(Object.entries(kind.schema.properties).filter(([name]) => !Object.prototype.hasOwnProperty.call(refs, name)))
  return shareFields({ ...kind.schema, properties } as ShareSchema, fields)
}

/** The kind as a guest sees it: only the named fields, no refs, policy,
 *  computed or transitions. */
export function guestKind(kind: RecordKind, fields: readonly string[]): RecordKind {
  const properties = Object.fromEntries(fields.map((f) => [f, kind.schema.properties[f]!]))
  return { title: kind.title, schema: { ...kind.schema, properties, required: (kind.schema.required ?? []).filter((f) => fields.includes(f)) } } as RecordKind
}

export interface GuestPage {
  mode: ShareMode
  title: string
  package: AppPackage
  /** A view link's snapshot, at every `records:` binding. */
  value?: { rows: Record<string, unknown>[]; count: number }
}

/** The one-template package a guest's host gets: nothing but the one kind's
 *  named fields. A view link's `template` (VAPP-246) reads only the snapshot:
 *  re-checked here, so a package that skipped install never reaches a guest;
 *  null = gone. Pure. */
export function guestPackage(published: Record<string, unknown>, share: PackageShare, kind: RecordKind, fields: readonly string[], title: string): AppPackage | null {
  const template = share.mode === `view` && share.template ? (published.templates as Record<string, Record<string, unknown>> | undefined)?.[share.template] : undefined
  if (share.mode === `view` && share.template && (!template || validatePackageShares({ ...published, share: [share] }).length > 0)) return null
  const node = share.mode === `submit` ? { id: `form`, component: `RecordForm`, kind: share.kind } : { id: `table`, component: `RecordTable`, kind: share.kind, editable: false, add: false }
  const pkg = {
    formatVersion: PACKAGE_FORMAT_VERSION,
    id: `share.${share.name}`,
    name: title,
    version: `1.0.0`,
    catalogId: template && typeof published.catalogId === `string` ? published.catalogId : CORE_CATALOG_ID,
    functions: share.mode === `submit` ? [`records.upsert`] : [],
    sources: [`records:${share.kind}`],
    records: { [share.kind]: guestKind(kind, fields) },
    templates: { main: template ? guestTemplate(template, share.kind) : { components: [{ id: `root`, component: `Page`, title, children: [node.id] }, node] } },
  } as unknown as AppPackage
  return template && validatePackage(pkg).length > 0 ? null : pkg
}

/** What a guest's page renders: the one-template package and a view link's
 *  snapshot. Null = gone. */
export async function guestPage(token: string, now = new Date()): Promise<GuestPage | null> {
  const live = await liveLink(token, now)
  if (!live) return null
  const { share, fields, app } = live
  const title = live.kind.title ?? app.name
  const pkg = guestPackage(app.package as Record<string, unknown>, share, live.kind, fields, title)
  if (!pkg) return null
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
  // `expand` reads other kinds: a guest never sees them.
  const { from: _from, expand: _expand, ...json } = query
  // A query without a limit reads the evaluator's default page, not the link's cap.
  const limit = typeof json.limit === `number` ? Math.min(json.limit, APP_SHARE_CAPS.viewRows) : APP_SHARE_CAPS.viewRows
  const out = serverRecordsValue(rows, share.kind, { ...json, limit }, { me: sponsor, names, today })
  const shown = ((out.value?.rows ?? []) as Record<string, unknown>[]).slice(0, APP_SHARE_CAPS.viewRows)
  return { rows: shown.map((row) => Object.fromEntries([`id`, `updatedAt`, ...fields].filter((k) => row[k] !== undefined).map((k) => [k, row[k]]))), count: shown.length }
}

/** The IP as the audit keeps it: a keyed hash, never the address. */
const ipHash = (request: Request) => createHmac(`sha256`, process.env.BETTER_AUTH_SECRET ?? `share`).update(clientIpFromRequest(request)).digest(`hex`)

const json = (status: number, body: Record<string, unknown>) => new Response(JSON.stringify(body), { status, headers: { "content-type": `application/json`, "cache-control": `no-store` } })

// Per replica, like the widget's limiter: the server reads and evaluates up to
// the whole kind per GET, so a loop on a view link is bounded.
const pageReads = new TokenBucketLimiter({ capacity: APP_SHARE_CAPS.viewPerIpPerHour, refillPerHour: APP_SHARE_CAPS.viewPerIpPerHour })

/** `GET /api/share/<token>`: what the guest's page renders. */
export async function handleSharePage(request: Request, token: string, now = new Date()): Promise<Response> {
  if (!pageReads.tryTake(clientIpFromRequest(request), now.getTime()).ok) return json(429, { error: APP_SHARE_COPY.tooMany })
  const page = await guestPage(token, now)
  return page ? json(200, page as unknown as Record<string, unknown>) : json(404, { error: APP_SHARE_COPY.gone })
}

/** The body as text, or null past `maxBodyBytes` without reading the rest. */
async function readCapped(request: Request): Promise<string | null> {
  if (Number(request.headers.get(`content-length`) ?? 0) > APP_SHARE_CAPS.maxBodyBytes) return null
  const reader = request.body?.getReader()
  if (!reader) return ``
  const chunks: Uint8Array[] = []
  let size = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) return Buffer.concat(chunks).toString(`utf8`)
    size += value.byteLength
    if (size > APP_SHARE_CAPS.maxBodyBytes) {
      await reader.cancel()
      return null
    }
    chunks.push(value)
  }
}

/** One submission's audit row, reserved under the link's lock BEFORE any
 *  write, so parallel requests cannot pass a cap; null = over a cap. A refusal
 *  never counts toward a cap, or one IP could close a link for everyone, and
 *  an IP keeps one refusal row an hour, so a flood cannot fill the table. */
async function takeSlot(link: LiveLink[`link`], hash: string, honeypot: boolean, now: Date): Promise<string | null> {
  const since = new Date(now.getTime() - HOUR_MS)
  const row = { linkId: link.id, appId: link.appId, teamId: link.teamId, ipHash: hash, createdAt: now }
  return db.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`app_share_link:${link.id}`}))`)
    const recent = and(eq(appShareSubmissions.linkId, link.id), gt(appShareSubmissions.createdAt, since))
    const accepted = and(recent, ne(appShareSubmissions.outcome, `rate_limited`))
    const [perLink] = await tx.select({ n: count() }).from(appShareSubmissions).where(accepted)
    const [perIp] = await tx.select({ n: count() }).from(appShareSubmissions).where(and(accepted, eq(appShareSubmissions.ipHash, hash)))
    if (underShareCaps(perIp?.n ?? 0, perLink?.n ?? 0)) {
      await tx.delete(appShareSubmissions).where(and(eq(appShareSubmissions.linkId, link.id), lt(appShareSubmissions.createdAt, new Date(now.getTime() - APP_SHARE_CAPS.auditDays * DAY_MS))))
      // `invalid` until the write says otherwise: a crash mid-write stays counted.
      const [slot] = await tx.insert(appShareSubmissions).values({ ...row, outcome: honeypot ? `honeypot` : `invalid` }).returning({ id: appShareSubmissions.id })
      return slot!.id
    }
    const [refused] = await tx.select({ n: count() }).from(appShareSubmissions).where(and(recent, eq(appShareSubmissions.ipHash, hash), eq(appShareSubmissions.outcome, `rate_limited`)))
    if (!refused?.n) await tx.insert(appShareSubmissions).values({ ...row, outcome: `rate_limited` })
    return null
  })
}

const settle = (slot: string, outcome: typeof appShareSubmissions.$inferInsert.outcome, recordId?: string) => db.update(appShareSubmissions).set({ outcome, recordId: recordId ?? null }).where(eq(appShareSubmissions.id, slot))

/** A value Postgres itself refuses (class 22, a NUL or a lone surrogate in
 *  jsonb): the guest's body, not a server fault. */
function isDataException(error: unknown): boolean {
  const code = (e: unknown) => (e as { code?: unknown } | null)?.code
  const pg = code((error as { cause?: unknown } | null)?.cause) ?? code(error)
  return typeof pg === `string` && pg.startsWith(`22`)
}

/** `POST /api/share/<token>`: one guest submission (app-share.json). */
export async function handleShareSubmit(request: Request, token: string, now = new Date()): Promise<Response> {
  const live = await liveLink(token, now)
  if (!live || live.share.mode !== `submit`) return json(404, { error: APP_SHARE_COPY.gone })
  const text = await readCapped(request)
  if (text === null) return json(413, { error: APP_SHARE_COPY.tooMany })
  const unreadable = json(400, { error: `The form could not be read` })
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return unreadable
  }
  if (typeof parsed !== `object` || parsed === null || Array.isArray(parsed)) return unreadable
  const body = parsed as { data?: unknown; website?: unknown }
  // The honeypot: answered like a success, nothing stored.
  const honeypot = typeof body.website === `string` && !!body.website
  const slot = await takeSlot(live.link, ipHash(request), honeypot, now)
  if (honeypot) return json(200, { ok: true })
  if (!slot) return json(429, { error: APP_SHARE_COPY.tooMany })
  const sent = typeof body.data === `object` && body.data !== null ? (body.data as Record<string, unknown>) : {}
  const data = Object.fromEntries(live.fields.filter((f) => Object.prototype.hasOwnProperty.call(sent, f) && sent[f] !== undefined).map((f) => [f, sent[f]]))
  try {
    const { id } = await upsertRecord(live.sponsor, { id: live.app.id }, { kind: live.share.kind, data, via: `share:${live.share.name}`, shareName: live.share.name })
    await settle(slot, `created`, id)
    return json(200, { ok: true })
  } catch (error) {
    if (error instanceof TRPCError && error.code === `BAD_REQUEST`) return json(400, { error: error.message })
    if (error instanceof TRPCError) {
      await settle(slot, `quota`)
      return json(403, { error: APP_SHARE_COPY.closed })
    }
    if (isDataException(error)) return unreadable
    throw error
  }
}
