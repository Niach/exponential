// VAPP-119: the `exp:` binding SOURCES beyond devices and records, the pure
// reference every app host mirrors (fixtures/app-sources.json is the spec;
// desktop `domain::app_sources`, iOS `AppSources.swift`, Android
// `AppSources.kt` replay its cases): what `exp:issues`, `exp:boards`,
// `exp:members`, `exp:sessions`, `exp:comments` and `exp:attachments` emit
// over the synced rows of a team.

import fixture from "../fixtures/app-sources.json" with { type: "json" }

export const APP_SOURCES_LIMITS = fixture.limits
/** The source names this module derives (`scheme:name` without `exp:`). */
export const APP_SOURCE_KINDS: readonly string[] = fixture.sources

export interface SourceBoard {
  id: string
  slug: string
  name: string
  prefix: string
  icon: string | null
  color: string | null
  sortOrder: number
  deletedAt: string | null
}
export interface SourceIssue {
  id: string
  boardId: string
  identifier: string
  title: string
  status: string
  priority: string | null
  assigneeId: string | null
  creatorId: string | null
  dueDate: string | null
  sortOrder: number
  createdAt: string
  updatedAt: string
}
export interface SourceMember {
  userId: string
  role: string
}
export interface SourceUser {
  id: string
  name: string
  email: string | null
  image: string | null
}
export interface SourceSession {
  id: string
  parentId: string | null
  agentId: string | null
  appId: string | null
  userId: string | null
  status: string
  busy: boolean
  needsInput: boolean
  title: string | null
  createdAt: string
  updatedAt: string
}
export interface SourceComment {
  id: string
  issueId: string
  authorId: string | null
  parentId: string | null
  source: string
  body: string
  createdAt: string
}
export interface SourceAttachment {
  id: string
  issueId: string | null
  commentId: string | null
  uploaderId: string | null
  filename: string
  contentType: string
  sizeBytes: number
  width: number | null
  height: number | null
  createdAt: string
}

/** The synced rows of ONE team, as the host reads them. */
export interface SourceStore {
  boards: readonly SourceBoard[]
  issues: readonly SourceIssue[]
  members: readonly SourceMember[]
  users: readonly SourceUser[]
  sessions: readonly SourceSession[]
  comments: readonly SourceComment[]
  attachments: readonly SourceAttachment[]
}

export interface SourceContext {
  /** The viewer's user id (null = signed out). */
  me: string | null
}

const text = (v: unknown): string => (v === null || v === undefined ? `` : String(v))
const cmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0)
const byId = <T extends { id: string }>(a: T, b: T) => cmp(a.id, b.id)

function limitOf(params: Record<string, string>): number {
  const n = Number(params.limit)
  return Number.isInteger(n) && n > 0 ? Math.min(n, APP_SOURCES_LIMITS.max) : APP_SOURCES_LIMITS.default
}

function userName(store: SourceStore, id: string | null): string {
  return (id && store.users.find((u) => u.id === id)?.name) || ``
}

/** A board by id or slug. */
function boardOf(store: SourceStore, ref: string | undefined): SourceBoard | undefined {
  if (!ref) return undefined
  return store.boards.find((b) => b.id === ref || b.slug === ref)
}

/** An issue by identifier (case-insensitive) or id. */
function issueOf(store: SourceStore, ref: string | undefined): SourceIssue | undefined {
  if (!ref) return undefined
  const upper = ref.toUpperCase()
  return store.issues.find((i) => i.id === ref || i.identifier.toUpperCase() === upper)
}

export function issuesValue(store: SourceStore, params: Record<string, string>, ctx: SourceContext) {
  const board = boardOf(store, params.board)
  if (params.board && !board) return { rows: [], count: 0 }
  const assignee = params.assignee === `me` ? ctx.me : params.assignee
  const search = params.search?.trim().toLowerCase()
  let rows = store.issues.filter((i) => {
    if (board && i.boardId !== board.id) return false
    if (params.status && i.status !== params.status) return false
    if (params.priority && text(i.priority) !== params.priority) return false
    if (params.assignee === `none` && i.assigneeId !== null) return false
    if (params.assignee && params.assignee !== `none` && i.assigneeId !== assignee) return false
    if (search && !i.title.toLowerCase().includes(search) && !i.identifier.toLowerCase().includes(search)) return false
    return true
  })
  const sort = params.sort === `updatedAt` || params.sort === `createdAt` ? params.sort : `sortOrder`
  const order = params.order === `desc` ? `desc` : params.order === `asc` ? `asc` : sort === `sortOrder` ? `asc` : `desc`
  rows = [...rows].sort((a, b) => {
    const c = sort === `sortOrder` ? a.sortOrder - b.sortOrder : cmp(a[sort], b[sort])
    return c !== 0 ? (order === `asc` ? c : -c) : byId(a, b)
  })
  const boards = new Map(store.boards.map((b) => [b.id, b]))
  return {
    rows: rows.slice(0, limitOf(params)).map((i) => ({
      id: i.id,
      identifier: i.identifier,
      title: i.title,
      // A Select / ChipInput option (shape `option`): bind the rows as its options.
      value: i.identifier,
      label: `${i.identifier} ${i.title}`,
      status: i.status,
      priority: i.priority,
      assignee: i.assigneeId,
      assigneeName: userName(store, i.assigneeId),
      board: i.boardId,
      boardSlug: boards.get(i.boardId)?.slug ?? ``,
      dueDate: i.dueDate,
      updatedAt: i.updatedAt,
    })),
    count: rows.length,
  }
}

export function boardsValue(store: SourceStore, params: Record<string, string>) {
  const rows = store.boards
    .filter((b) => b.deletedAt === null)
    .sort((a, b) => a.sortOrder - b.sortOrder || cmp(a.name, b.name) || byId(a, b))
  return {
    rows: rows.slice(0, limitOf(params)).map((b) => ({ id: b.id, slug: b.slug, name: b.name, prefix: b.prefix, icon: b.icon, color: b.color, value: b.id, label: b.name })),
    count: rows.length,
  }
}

export function membersValue(store: SourceStore, params: Record<string, string>, ctx: SourceContext) {
  const rows = store.members
    .map((m) => {
      const u = store.users.find((u) => u.id === m.userId)
      const name = u?.name ?? ``
      return { id: m.userId, name, email: u?.email ?? null, avatar: u?.image ?? null, role: m.role, value: m.userId, label: name }
    })
    .sort((a, b) => cmp(a.name, b.name) || byId(a, b))
  return { rows: rows.slice(0, limitOf(params)), count: rows.length, me: ctx.me }
}

export function sessionsValue(store: SourceStore, params: Record<string, string>, ctx: SourceContext) {
  const rows = store.sessions
    .filter((s) => s.parentId === null && s.appId === null)
    .filter((s) => !params.status || s.status === params.status)
    .filter((s) => !(params.mine === `1` || params.mine === `true`) || (ctx.me !== null && s.userId === ctx.me))
    .sort((a, b) => -cmp(a.updatedAt, b.updatedAt) || byId(a, b))
  return {
    rows: rows.slice(0, limitOf(params)).map((s) => ({
      id: s.id,
      title: s.title ?? ``,
      status: s.status,
      busy: s.busy,
      needsInput: s.needsInput,
      kind: s.agentId ? `agent` : `thread`,
      updatedAt: s.updatedAt,
    })),
    count: rows.length,
  }
}

export function commentsValue(store: SourceStore, params: Record<string, string>) {
  const issue = issueOf(store, params.issue)
  if (!issue) return null
  const rows = store.comments.filter((c) => c.issueId === issue.id).sort((a, b) => cmp(a.createdAt, b.createdAt) || byId(a, b))
  return {
    rows: rows.slice(0, limitOf(params)).map((c) => ({
      id: c.id,
      body: c.body,
      author: c.authorId,
      authorName: userName(store, c.authorId),
      parent: c.parentId,
      source: c.source,
      createdAt: c.createdAt,
    })),
    count: rows.length,
  }
}

export function attachmentsValue(store: SourceStore, params: Record<string, string>) {
  const issue = issueOf(store, params.issue)
  if (!issue) return null
  const rows = store.attachments.filter((a) => a.issueId === issue.id).sort((a, b) => cmp(a.createdAt, b.createdAt) || byId(a, b))
  return {
    rows: rows.slice(0, limitOf(params)).map((a) => ({
      id: a.id,
      name: a.filename,
      type: a.contentType,
      size: a.sizeBytes,
      url: `/api/attachments/${a.id}`,
      width: a.width,
      height: a.height,
      uploader: a.uploaderId,
      uploaderName: userName(store, a.uploaderId),
      createdAt: a.createdAt,
    })),
    count: rows.length,
  }
}

/** One `exp:<name>?params` over the store: the value the host emits at the
 *  bound path (null for a name this module does not derive, or a comments /
 *  attachments source without a resolvable issue). */
export function sourceValue(name: string, params: Record<string, string>, store: SourceStore, ctx: SourceContext): unknown {
  switch (name) {
    case `issues`:
      return issuesValue(store, params, ctx)
    case `boards`:
      return boardsValue(store, params)
    case `members`:
      return membersValue(store, params, ctx)
    case `sessions`:
      return sessionsValue(store, params, ctx)
    case `comments`:
      return commentsValue(store, params)
    case `attachments`:
      return attachmentsValue(store, params)
    default:
      return null
  }
}
