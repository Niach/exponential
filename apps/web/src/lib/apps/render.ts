import { and, desc, eq, isNull, or } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  ExponentialHost,
  reduceSurface,
  bridgePackage,
  describeSurface,
  resolveScreen,
  screenData,
  navScreens,
  packageScreens,
  type AppPackage,
  type ParsedSource,
  type SourceEmit,
  type UiNode,
} from "@exponential_app/eui"
import { RECORDS_SCHEME, parseRecordsQuery, recordsValue, type RecordRow } from "@exponential_app/eui/host/query"
import { APP_SOURCE_KINDS, sourceValue, type SourceStore } from "@exp/domain-contract/app-sources"
import { UPSTREAM_BINDING } from "@exp/emcp/upstream"
import { db as defaultDb } from "@/db/connection"
import { attachments, boards, comments, issues, sessions, teamMembers, teams, users, type App } from "@/db/schema"
import { boardVisible, issueVisible } from "@/lib/board-visibility"
import { assertTeamMember } from "@/lib/team-membership"
import { resolveTeamUserRef } from "@/lib/team-user-ref"
import { resolveZone, zoneDay, zoneOffset } from "@exp/domain-contract/team-zone"
import { appExtension } from "./package"
import { appAccessLevel, loadApp, readAppRecordRows, readRefLookups, teamMemberNames } from "./records-service"

// #FEED-218: an app screen RESOLVED on the server, as one viewer sees it:
// the package (head or published), the screen and its params, every binding
// of its template followed (`exp:` over the team's rows through the same
// reference `sourceValue` the four clients replay, `records:` through the
// records service under the viewer's read policy), then bound and described
// (`describeSurface`). MCP `exponential_apps {op: "render"}` returns the text;
// the screenshot op (#FEED-244) ships the same snapshot to a device painter.

export const RENDER_FORM_FACTORS = [`phone`, `desktop`] as const
export type RenderFormFactor = (typeof RENDER_FORM_FACTORS)[number]

export interface ResolveAppScreenInput {
  /** The caller (MCP / tRPC session user). */
  userId: string
  teamId: string
  slug: string
  screen?: string
  params?: Record<string, string>
  version?: `head` | `published`
  /** Run as another member (owners only; user id, member id or email). */
  asUser?: string
}

export interface ResolvedAppScreen {
  app: Pick<App, `id` | `teamId` | `slug` | `name` | `headVersion` | `publishedVersion`>
  pkg: AppPackage
  version: number
  screen: { template: string; title: string | null; params: Record<string, string> }
  /** The navigation a host draws (phones: tabs, wide frames: sidebar rows). */
  nav: { template: string; title: string }[]
  templateId: string
  /** The surface's data model with every binding followed. */
  data: Record<string, unknown>
  /** The reduced tree as a renderer paints it (macros expanded) + its item templates. */
  root: UiNode | null
  templates: Record<string, UiNode>
  /** The same tree before macro expansion: the components the author wrote. */
  authored: { root: UiNode | null; templates: Record<string, UiNode> }
  viewer: { id: string; role: string; access: string }
  timeZone: string | null
  issues: string[]
}

type Executor = typeof defaultDb

/** The tables an `exp:` source reads (the web host's SOURCE_TABLES). */
const NEEDS: Record<string, (keyof SourceStore)[]> = {
  issues: [`issues`, `boards`, `users`],
  boards: [`boards`],
  members: [`members`, `users`],
  me: [`members`, `users`],
  sessions: [`sessions`],
  comments: [`issues`, `comments`, `users`],
  attachments: [`issues`, `attachments`, `users`],
}

const iso = (v: Date | string | null | undefined): string | null => (v == null ? null : v instanceof Date ? v.toISOString() : String(v))
const day = (v: Date | string | null | undefined): string | null => (v == null ? null : v instanceof Date ? v.toISOString().slice(0, 10) : String(v).slice(0, 10))

/** One team's rows as a client's synced store holds them, only the tables
 *  `wanted` names (empty for the rest). Sessions follow the visibility rule
 *  (shared, or the viewer's own). */
export async function serverSourceStore(teamId: string, viewerId: string, wanted: ReadonlySet<keyof SourceStore>, executor: Executor = defaultDb): Promise<SourceStore> {
  const want = (t: keyof SourceStore) => wanted.has(t)
  const [b, i, m, s, c, a] = await Promise.all([
    want(`boards`)
      ? executor.select().from(boards).where(and(eq(boards.teamId, teamId), boardVisible()))
      : [],
    want(`issues`)
      ? executor.select().from(issues).innerJoin(boards, eq(boards.id, issues.boardId)).where(and(eq(issues.teamId, teamId), boardVisible(), issueVisible()))
      : [],
    want(`members`) || want(`users`)
      ? executor.select({ userId: teamMembers.userId, role: teamMembers.role, name: users.name, email: users.email, image: users.image }).from(teamMembers).innerJoin(users, eq(users.id, teamMembers.userId)).where(eq(teamMembers.teamId, teamId))
      : [],
    want(`sessions`)
      ? executor.select().from(sessions).where(and(eq(sessions.teamId, teamId), or(eq(sessions.shared, true), eq(sessions.userId, viewerId)))).orderBy(desc(sessions.updatedAt)).limit(2000)
      : [],
    want(`comments`)
      ? executor.select().from(comments).where(and(eq(comments.teamId, teamId), isNull(comments.boardDeletedAt), isNull(comments.boardArchivedAt)))
      : [],
    want(`attachments`)
      ? executor.select().from(attachments).where(and(eq(attachments.teamId, teamId), isNull(attachments.boardDeletedAt), isNull(attachments.boardArchivedAt)))
      : [],
  ])
  return {
    boards: b.map((r) => ({ id: r.id, slug: r.slug, name: r.name, prefix: r.prefix, icon: r.icon, color: r.color, sortOrder: r.sortOrder, deletedAt: iso(r.deletedAt) })),
    issues: i.map(({ issues: r }) => ({
      id: r.id,
      boardId: r.boardId,
      identifier: r.identifier,
      title: r.title,
      status: r.status,
      priority: r.priority,
      assigneeId: r.assigneeId,
      creatorId: r.creatorId,
      dueDate: day(r.dueDate),
      sortOrder: r.sortOrder,
      createdAt: iso(r.createdAt)!,
      updatedAt: iso(r.updatedAt)!,
    })),
    members: m.map((r) => ({ userId: r.userId, role: r.role })),
    users: m.map((r) => ({ id: r.userId, name: r.name ?? ``, email: r.email, image: r.image })),
    sessions: s.map((r) => ({
      id: r.id,
      parentId: r.parentId,
      agentId: r.agentId,
      appId: r.appId,
      userId: r.userId,
      status: r.status,
      busy: r.busy,
      needsInput: r.needsInput,
      title: r.title,
      createdAt: iso(r.createdAt)!,
      updatedAt: iso(r.updatedAt)!,
    })),
    comments: c.map((r) => ({ id: r.id, issueId: r.issueId, authorId: r.authorId, parentId: r.parentId, source: r.source, body: r.body, createdAt: iso(r.createdAt)! })),
    attachments: a.map((r) => ({
      id: r.id,
      issueId: r.issueId,
      commentId: r.commentId,
      uploaderId: r.uploaderId,
      filename: r.filename,
      contentType: r.contentType,
      sizeBytes: r.sizeBytes,
      width: r.width,
      height: r.height,
      createdAt: iso(r.createdAt)!,
    })),
  } as SourceStore
}

/** The screen, resolved and bound as the viewer (see the file comment). */
export async function resolveAppScreen(input: ResolveAppScreenInput, executor: Executor = defaultDb): Promise<ResolvedAppScreen> {
  const caller = await assertTeamMember(input.userId, input.teamId)
  const app = await loadApp(executor, { slug: input.slug, teamId: input.teamId })
  // The caller's own access first: none = the app does not exist for them.
  if ((await appAccessLevel(executor, app.id, input.userId)) === `none`) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  let viewerId = input.userId
  let role = caller.role
  if (input.asUser !== undefined) {
    // VAPP-198: Run as another member = owners only.
    if (caller.role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner may render as another member` })
    viewerId = await resolveTeamUserRef(app.teamId, input.asUser)
    role = (await assertTeamMember(viewerId, app.teamId)).role
  }
  const access = await appAccessLevel(executor, app.id, viewerId)
  if (access === `none`) throw new TRPCError({ code: `NOT_FOUND`, message: `That member has no access to this app` })

  const stored = input.version === `published` ? app.package : (app.draftPackage ?? app.package)
  if (!stored) throw new TRPCError({ code: `BAD_REQUEST`, message: input.version === `published` ? `The app is not published yet` : `The app has no version yet` })
  const pkg = bridgePackage(stored as unknown as AppPackage)
  const version = input.version === `published` ? app.publishedVersion : app.headVersion
  const screen = resolveScreen(pkg as { screens?: unknown }, input.screen ?? null, input.params ?? null)
  const templateIds = Object.keys((pkg as { templates?: Record<string, unknown> }).templates ?? {})
  const declared = packageScreens(pkg as { screens?: unknown }).map((s) => s.template)
  // Without screens any template may be named (the app is its first one).
  const bare = declared.length === 0 && input.screen !== undefined && templateIds.includes(input.screen)
  if (input.screen && screen?.screen !== input.screen && !bare) {
    const names = declared.length ? declared : templateIds
    throw new TRPCError({ code: `BAD_REQUEST`, message: `No screen ${input.screen}${names.length ? `: ${declared.length ? `screens` : `templates`} are ${names.join(`, `)}` : ``}` })
  }
  const templateId = bare ? input.screen! : (screen?.screen ?? templateIds[0])
  if (!templateId) throw new TRPCError({ code: `BAD_REQUEST`, message: `The package has no template` })

  const [team] = await executor.select({ timeZone: teams.timeZone }).from(teams).where(eq(teams.id, app.teamId)).limit(1)
  const zoneName = resolveZone(team?.timeZone)
  const zone = zoneName ? (ms: number) => zoneOffset(zoneName, ms) : undefined
  const issues: string[] = []
  const pending: Promise<void>[] = []
  let store: Promise<SourceStore> | null = null
  const wantedTables = new Set<keyof SourceStore>()

  // `exp:` values over one lazily read store (the tables every bind names).
  const expSource = (source: ParsedSource, emit: SourceEmit) => {
    if (!APP_SOURCE_KINDS.includes(source.name)) {
      issues.push(`${source.uri}: ${source.name === `devices` ? `exp:devices is read on the device only (empty here)` : `no such source`}`)
      emit(null)
      return
    }
    for (const t of NEEDS[source.name] ?? []) wantedTables.add(t)
    pending.push(
      Promise.resolve().then(async () => {
        store ??= serverSourceStore(app.teamId, viewerId, wantedTables, executor)
        emit(sourceValue(source.name, source.params, await store, { me: viewerId, access }))
      })
    )
  }
  const names = teamMemberNames(app.teamId, executor)
  const recordsSource = (source: ParsedSource, emit: SourceEmit) => {
    const query = parseRecordsQuery(source.params, source.name)
    pending.push(
      (async () => {
        try {
          const target = query.app ? await loadApp(executor, { slug: query.app, teamId: app.teamId }) : app
          const kind = query.from ?? source.name
          const { rows } = await readAppRecordRows(target, kind, {}, viewerId, executor)
          const lookups = await readRefLookups(target, kind, { expand: query.expand }, rows, viewerId, executor)
          const ms = Date.now()
          const value = recordsValue(lookups.targets.length ? [...rows, ...lookups.targets] : (rows as RecordRow[]), query, {
            me: viewerId,
            names: await names,
            now: new Date(ms).toISOString(),
            today: zone ? zoneDay(zone, ms) : undefined,
            refs: lookups.refs,
            issues: lookups.issues,
            transitions: lookups.transitions ?? null,
            role: lookups.role ?? null,
          })
          emit(value)
        } catch (error) {
          issues.push(`${source.uri}: ${error instanceof Error ? error.message : String(error)}`)
          emit(null)
        }
      })()
    )
  }

  const host = new ExponentialHost({
    me: viewerId,
    zone,
    extensions: [appExtension as never],
    packages: [pkg],
    sources: {
      exp: expSource,
      [RECORDS_SCHEME]: recordsSource,
      [UPSTREAM_BINDING.scheme]: (source, emit) => {
        issues.push(`${source.uri}: upstream bindings call the member's MCP server live; not rendered here`)
        emit(null)
      },
    },
  })
  const surfaceId = `render`
  host.applyTemplate({ surfaceId, templateId, packageId: pkg.id, ...(screen ? { data: screenData(screen) } : {}) })
  // A bind may start another (bind() reads one source's value): settle until quiet.
  for (let round = 0; round < 5 && pending.length; round++) await Promise.all(pending.splice(0))
  const surface = host.surface(surfaceId)
  for (const i of host.issues) issues.push(`${i.code}: ${i.message}${i.path ? ` (${i.path})` : ``}`)
  for (const i of surface?.issues ?? []) issues.push(`${i.message}${i.path ? ` (${i.path})` : ``}`)
  const authored = surface?.components.length ? reduceSurface(surface.components, { catalogId: surface.catalogId, extensions: [appExtension as never], expand: false, validate: false }) : null
  return {
    app: { id: app.id, teamId: app.teamId, slug: app.slug, name: app.name, headVersion: app.headVersion, publishedVersion: app.publishedVersion },
    pkg,
    version,
    screen: { template: templateId, title: screen?.title ?? null, params: screen?.params ?? {} },
    nav: navScreens(pkg as { screens?: unknown }).map((s) => ({ template: s.template, title: s.title })),
    templateId,
    data: surface?.data ?? {},
    root: surface?.root ?? null,
    templates: surface?.templates ?? {},
    authored: { root: authored?.root ?? null, templates: authored?.templates ?? {} },
    viewer: { id: viewerId, role, access },
    timeZone: zoneName ?? null,
    issues,
  }
}

export interface RenderedAppScreen {
  screen: string
  title: string | null
  version: number
  formFactor: RenderFormFactor
  viewer: { id: string; role: string; access: string }
  nav: string
  outline: string
  issues: string[]
}

/** MCP `apps render`: the resolved screen as text. */
export async function renderAppScreen(input: ResolveAppScreenInput & { formFactor?: RenderFormFactor }, executor: Executor = defaultDb): Promise<RenderedAppScreen> {
  const resolved = await resolveAppScreen(input, executor)
  const formFactor = input.formFactor ?? `desktop`
  const zoneName = resolved.timeZone
  // The outline names what the author wrote (a Row, not the Box it expands to).
  const described = resolved.authored.root
    ? describeSurface(resolved.authored.root, resolved.data, { templates: resolved.authored.templates, extensions: [appExtension as never], now: () => Date.now(), ...(zoneName ? { zone: (ms: number) => zoneOffset(zoneName, ms) } : {}) })
    : { outline: `(no surface)`, issues: [] }
  const navItems = resolved.nav.map((s) => (s.template === resolved.templateId ? `[${s.title}]` : s.title))
  // The host draws the navigation: a phone's tab bar, a wide frame's sidebar.
  const nav = navItems.length ? `${formFactor === `phone` ? `tab bar` : `sidebar`}: ${navItems.join(` · `)}` : `no navigation (one screen)`
  return {
    screen: resolved.templateId,
    title: resolved.screen.title,
    version: resolved.version,
    formFactor,
    viewer: resolved.viewer,
    nav,
    outline: described.outline,
    issues: [...resolved.issues, ...described.issues],
  }
}
