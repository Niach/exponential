// #VAPP-292 — a stand-in Exponential MCP server for proving the genUI view in
// a REAL MCP Apps host (the ext-apps basic-host, or Claude through a tunnel)
// without the database: the same tool names, `_meta.ui` bindings, resource
// and answers' shapes as apps/web lib/emcp, over an in-memory team running
// the Standup sample app (samples/apps/standup.app.json).
//
//   bun run build                                   # the two documents
//   bun scripts/connector-harness.ts                # http://localhost:3001/mcp
//
// Then point the basic host at it (SERVERS='["http://localhost:3001/mcp"]')
// and call exponential_apps {op: "get", slug: "standup"} or exponential_show.
// The issue, device and inbox views ride the same harness (app.html): their
// nouns answer list ops the way /api/mcp does outside a session (the model's
// summary + structuredContent, lib/emcp/view-results.ts).

import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { randomUUID } from "node:crypto"
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { z } from "zod"
import { inlineShowResult } from "../src/genui/result"
import { compactViewResult } from "../../../apps/web/src/lib/emcp/view-results"

const PORT = Number(process.env.PORT ?? 3001)
const ORIGIN = process.env.ORIGIN ?? `http://localhost:${PORT}`
const WEB_PUBLIC = fileURLToPath(new URL(`../../../apps/web/public/`, import.meta.url))
const standup = JSON.parse(readFileSync(fileURLToPath(new URL(`../../../samples/apps/standup.app.json`, import.meta.url)), `utf8`))

const TEAM = `5b0c2f7e-0000-4000-8000-000000000001`
const ME = `user-danny`
const MEMBERS = [
  { id: ME, memberId: `m1`, name: `Danny`, email: `danny@exponential.at`, image: null, role: `owner`, placeholder: false },
  { id: `user-ana`, memberId: `m2`, name: `Ana`, email: `ana@example.com`, image: null, role: `member`, placeholder: false },
]
const APP = { id: `5b0c2f7e-0000-4000-8000-0000000000aa`, slug: `standup`, name: `Standup`, teamId: TEAM, icon: `users`, color: `#f59e0b`, headVersion: 1, publishedVersion: 1, grantedTools: [] as string[] }
const day = (offset: number) => new Date(Date.now() + offset * 86_400_000).toISOString().slice(0, 10)
const RECORDS: Array<Record<string, unknown>> = [
  { id: randomUUID(), kind: `update`, data: { date: day(0), yesterday: `Wired the genUI view into MCP Apps`, today: `Screenshots in the basic host`, blockers: null }, createdBy: `user-ana`, authorId: `user-ana`, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
  { id: randomUUID(), kind: `update`, data: { date: day(-1), yesterday: `Audited OAuth + DCR for claude.ai`, today: `The inline show card`, blockers: `Waiting on the tunnel` }, createdBy: ME, authorId: ME, createdAt: new Date(Date.now() - 86_400_000).toISOString(), updatedAt: new Date(Date.now() - 86_400_000).toISOString() },
]

const ok = (data: unknown) => ({ content: [{ type: `text` as const, text: JSON.stringify(data, null, 2) }] })
const err = (message: string) => ({ isError: true, content: [{ type: `text` as const, text: message }] })
const GENUI = { ui: { resourceUri: `ui://exponential/genui` } }
const noun = { op: z.string() }

function genuiHtml(file = `genui.html`): string {
  const html = readFileSync(`${WEB_PUBLIC}mcp-apps/${file}`, `utf8`)
  // The same stamping as apps/web lib/emcp/apps.ts `mcpAppHtml`.
  return html
    .replace(/<meta name="exp-origin" content="[^"]*"\s*\/?>/, `<meta name="exp-origin" content="${ORIGIN}" />`)
    .replace(/url\((["']?)\/fonts\//g, (_, quote: string) => `url(${quote}${ORIGIN}/fonts/`)
}

// ── The tracker stand-in (issues, devices, inbox) ──────────────────────────
const BOARD = `5b0c2f7e-0000-4000-8000-0000000000b1`
const STATUSES = [`backlog`, `backlog`, `in_progress`, `in_review`, `backlog`, `done`] as const
const PRIORITIES = [`urgent`, `high`, `medium`, `low`, `none`] as const
const TITLES = [`Board view: keyboard navigation between rows`, `Inbox: mark all read from the header`, `Devices: show the account usage in the card`, `MCP: page long issue lists`, `Sessions: resume an ended child as a sibling`, `Apps: records history restore`, `Desktop: crash on resume from sleep`, `Android: call service refusal`, `iOS: dark mode contrast on badges`, `Docs: interactive connector guide`]
const ISSUES = Array.from({ length: 64 }, (_, i) => ({
  id: `5b0c2f7e-0000-4000-8000-${String(1000 + i).padStart(12, `0`)}`,
  identifier: `EXP-${1800 - i}`,
  title: `${TITLES[i % TITLES.length]}${i >= TITLES.length ? ` (${Math.floor(i / TITLES.length) + 1})` : ``}`,
  description: `The ${TITLES[i % TITLES.length].toLowerCase()} needs a pass.\n\n- matches the web\n- same on every platform`,
  status: STATUSES[i % STATUSES.length],
  statusId: null,
  priority: PRIORITIES[i % PRIORITIES.length],
  boardId: BOARD,
  teamId: TEAM,
  assigneeId: i % 3 === 0 ? `user-ana` : i % 3 === 1 ? ME : null,
  dueDate: i % 7 === 0 ? day(i % 2 ? 3 : -2) : null,
  prNumber: i % 5 === 3 ? 2600 + i : null,
  prState: i % 5 === 3 ? `open` : null,
  createdAt: new Date(Date.now() - i * 3_600_000).toISOString(),
  updatedAt: new Date(Date.now() - i * 1_800_000).toISOString(),
}))
const issueDetail = (issue: (typeof ISSUES)[number]) => ({
  ...issue,
  url: `${ORIGIN}/t/acme/boards/exp/issues/${issue.identifier}`,
  labelIds: [],
  relations: [],
  attachmentCount: 0,
  recentComments: [
    { id: randomUUID(), authorId: `user-ana`, parentId: null, source: `user`, body: `Looks right on web; checking the phones next.`, createdAt: new Date(Date.now() - 7_200_000).toISOString(), editedAt: null },
  ],
})
const usage = (week: number, model: number, hours: number) => ({
  fetchedAt: new Date().toISOString(),
  windows: [
    { key: `session`, label: `5h`, percent: Math.round(week / 3), resetsAt: new Date(Date.now() + 2 * 3_600_000).toISOString() },
    { key: `weekly`, label: `Week`, percent: week, resetsAt: new Date(Date.now() + hours * 3_600_000).toISOString() },
    { key: `model:fable`, label: `Fable`, percent: model, resetsAt: new Date(Date.now() + hours * 3_600_000).toISOString() },
  ],
})
const profiles = (dead: boolean) => [
  { id: `p-ana`, signedIn: true, active: true, health: `ok`, email: `ana@example.com`, plan: `max`, usage: usage(7, 9, 89) },
  { id: `p-ops`, signedIn: true, health: `ok`, email: `ops@example.com`, plan: `max`, usage: usage(37, 0, 134) },
  { id: `p-lab`, signedIn: true, health: dead ? `needs_relogin` : `ok`, email: `lab@example.com`, plan: `max`, usage: usage(100, 97, 14) },
]
const DEVICES = [
  { deviceId: `dev-macbook`, label: `macbook`, kind: `desktop`, platform: `macos`, online: true, lastSeenAt: new Date().toISOString(), agents: [`claude`, `codex`], unauthedAgents: [], caps: [`agent-login`], version: `0.14.150`, isDefault: true, sharedTeamIds: [], agentAccounts: { claude: { signedIn: true, profiles: profiles(true) }, codex: { signedIn: true, profiles: [{ id: `p-codex`, signedIn: true, active: true, health: `ok`, email: `ana@example.com`, plan: `pro` }] } } },
  { deviceId: `dev-mint`, label: `mint`, kind: `desktop`, platform: `linux`, online: true, lastSeenAt: new Date().toISOString(), agents: [`claude`], unauthedAgents: [], caps: [`agent-login`], version: `0.14.150`, sharedTeamIds: [], agentAccounts: { claude: { signedIn: true, profiles: profiles(false) } } },
  { deviceId: `dev-server`, label: `server`, kind: `server`, platform: `linux`, online: true, lastSeenAt: new Date().toISOString(), agents: [`claude`, `codex`], unauthedAgents: [], caps: [`agent-login`], version: `0.14.150`, sharedTeamIds: [TEAM], agentAccounts: { claude: { signedIn: true, profiles: profiles(false).slice(0, 1) } } },
  { deviceId: `dev-old`, label: `old-laptop`, kind: `desktop`, platform: `windows`, online: false, lastSeenAt: new Date(Date.now() - 9 * 86_400_000).toISOString(), agents: [`claude`], unauthedAgents: [], caps: [], version: `0.13.90`, sharedTeamIds: [], agentAccounts: { claude: { signedIn: true, profiles: profiles(false).slice(1, 2) } } },
  { deviceId: `dev-ana`, label: `ana-build`, kind: `server`, platform: `linux`, online: true, lastSeenAt: new Date().toISOString(), agents: [`claude`], unauthedAgents: [], caps: [], version: `0.14.150`, owner: { id: `user-ana`, name: `Ana` }, agentAccounts: { claude: { signedIn: true, profiles: profiles(false).slice(0, 1) } } },
]
const NOTIFICATIONS = ISSUES.slice(0, 6).map((issue, i) => ({ id: randomUUID(), userId: ME, issueId: issue.id, teamId: TEAM, type: i % 2 ? `issue_comment` : `issue_assigned`, title: `${issue.identifier} ${issue.title}`, body: i % 2 ? `Ana: Looks right on web.` : null, readAt: i > 3 ? new Date().toISOString() : null, pushedAt: null, createdAt: new Date(Date.now() - i * 3_600_000).toISOString(), updatedAt: new Date().toISOString() }))

const VIEWS = { issues: `ui://exponential/issues`, inbox: `ui://exponential/inbox`, devices: `ui://exponential/devices` } as const
const viewHtml = (view: keyof typeof VIEWS) => genuiHtml(`app.html`).replace(/<meta name="exp-view" content="[a-z]+"\s*\/?>/, `<meta name="exp-view" content="${view}" />`)
const num = (value: unknown, fallback: number) => (typeof value === `number` ? value : typeof value === `string` && /^\d+$/.test(value) ? Number(value) : fallback)
/** A list op the way /api/mcp answers it outside a session. */
const listed = (noun: string, rows: unknown[], args: Record<string, unknown>) => {
  const limit = num(args.limit, 50)
  const offset = num(args.offset, 0)
  return compactViewResult(noun, { limit, offset }, ok(rows.slice(offset, offset + limit)))
}
const findIssue = (id: unknown) => ISSUES.find((issue) => issue.id === id || issue.identifier === id)

function registerTracker(s: McpServer) {
  for (const view of Object.keys(VIEWS) as Array<keyof typeof VIEWS>) {
    s.registerResource(`exponential-${view}-view`, VIEWS[view], { title: `Exponential ${view}`, mimeType: `text/html;profile=mcp-app` }, async () => ({
      contents: [{ uri: VIEWS[view], mimeType: `text/html;profile=mcp-app`, text: viewHtml(view), _meta: { ui: { prefersBorder: true, csp: { resourceDomains: [ORIGIN, `https://fonts.googleapis.com`, `https://fonts.gstatic.com`] } } } }],
    }))
  }
  s.registerTool(`exponential_issues`, { description: `Issues: list, show, get, create, update, delete, subscribe.`, inputSchema: z.object(noun).passthrough(), _meta: { ui: { resourceUri: VIEWS.issues } } }, async (args: Record<string, unknown>) => {
    switch (args.op) {
      case `list`:
      case `show`: {
        const open = ISSUES.filter((issue) => args.includeClosed === true || args.includeClosed === `true` || issue.status !== `done`)
        return listed(`issues`, open, args)
      }
      case `get`: {
        const issue = findIssue(args.id)
        return issue ? ok(issueDetail(issue)) : err(`Issue not found`)
      }
      case `create`: {
        const issue = { ...ISSUES[0]!, id: randomUUID(), identifier: `EXP-${1801 + ISSUES.length - 64}`, title: String(args.title ?? `New issue`), status: `backlog` as const, createdAt: new Date().toISOString() }
        ISSUES.unshift(issue)
        return ok(issue)
      }
      case `update`:
      case `update_status`: {
        const issue = findIssue(args.id)
        if (!issue) return err(`Issue not found`)
        if (typeof args.status === `string`) (issue as { status: string }).status = args.status
        if (typeof args.title === `string`) issue.title = args.title
        return ok(issue)
      }
      default:
        return ok({ ok: true })
    }
  })
  s.registerTool(`exponential_devices`, { description: `Devices: list, account_login.`, inputSchema: z.object(noun).passthrough(), _meta: { ui: { resourceUri: VIEWS.devices } } }, async (args: Record<string, unknown>) =>
    args.op === `list` ? listed(`devices`, DEVICES, args) : ok({ status: `url`, commandId: randomUUID(), url: `https://claude.ai/oauth`, code: null })
  )
  s.registerTool(`exponential_notifications`, { description: `Notifications: list, mark_read.`, inputSchema: z.object(noun).passthrough(), _meta: { ui: { resourceUri: VIEWS.inbox } } }, async (args: Record<string, unknown>) =>
    args.op === `list` ? listed(`notifications`, NOTIFICATIONS, args) : ok({ ok: true })
  )
  s.registerTool(`exponential_boards`, { description: `Boards.`, inputSchema: z.object(noun).passthrough() }, async () => ok({ id: BOARD, teamId: TEAM, name: `Exponential`, slug: `exp` }))
  s.registerTool(`exponential_comments`, { description: `Comments.`, inputSchema: z.object(noun).passthrough() }, async (args: Record<string, unknown>) =>
    args.op === `create` ? ok({ id: randomUUID(), authorId: ME, parentId: null, source: `user`, body: String(args.body ?? ``), createdAt: new Date().toISOString(), editedAt: null }) : ok([])
  )
  s.registerTool(`exponential_attachments`, { description: `Attachments.`, inputSchema: z.object(noun).passthrough() }, async () => ok([]))
}

function server(): McpServer {
  const s = new McpServer({ name: `exponential`, version: `0.1.0` })
  s.registerResource(`exponential-genui-view`, `ui://exponential/genui`, { title: `Exponential`, mimeType: `text/html;profile=mcp-app` }, async () => ({
    contents: [{ uri: `ui://exponential/genui`, mimeType: `text/html;profile=mcp-app`, text: genuiHtml(), _meta: { ui: { prefersBorder: true, csp: { resourceDomains: [ORIGIN, `https://fonts.googleapis.com`, `https://fonts.gstatic.com`] } } } }],
  }))
  s.registerTool(`exponential_show`, { description: `ONE card in your chat (outside a session: drawn inline).`, inputSchema: { surface: z.any().optional(), title: z.string().optional() }, _meta: GENUI }, async ({ surface, title }) =>
    surface ? ok(inlineShowResult({ kind: `surface`, surface, title: title ?? null })) : err(`Pass surface.`)
  )
  s.registerTool(`exponential_apps`, { description: `Exponential apps: list, get, records_list, records_upsert, records_delete.`, inputSchema: z.object(noun).passthrough(), _meta: GENUI }, async (args: Record<string, unknown>) => {
    switch (args.op) {
      case `list`:
        return ok([APP])
      case `get`:
        if (args.slug !== APP.slug) return err(`App not found`)
        return ok({ ...APP, url: `${ORIGIN}/t/acme/apps/standup`, version: 1, package: standup.package, linkableScreens: [], viewer: { id: ME, access: `write` } })
      case `records_list`:
        return ok({ records: RECORDS.filter((r) => !args.kind || r.kind === args.kind) })
      case `records_upsert`: {
        const now = new Date().toISOString()
        if (typeof args.id === `string`) {
          const row = RECORDS.find((r) => r.id === args.id)
          if (!row) return err(`Record not found`)
          row.data = { ...(row.data as object), ...(args.data as object) }
          row.updatedAt = now
          return ok({ ok: true, id: row.id, created: false })
        }
        const id = randomUUID()
        RECORDS.unshift({ id, kind: args.kind, data: args.data, createdBy: ME, authorId: ME, createdAt: now, updatedAt: now })
        return ok({ ok: true, id, created: true })
      }
      case `records_delete`: {
        const at = RECORDS.findIndex((r) => r.id === args.id)
        if (at >= 0) RECORDS.splice(at, 1)
        return ok({ ok: true, id: args.id, deleted: true })
      }
      default:
        return ok({ ok: true })
    }
  })
  registerTracker(s)
  s.registerTool(`exponential_people`, { description: `A team's members.`, inputSchema: z.object(noun).passthrough() }, async () => ok(MEMBERS))
  s.registerTool(`exponential_teams`, { description: `Teams.`, inputSchema: z.object(noun).passthrough() }, async () => ok({ id: TEAM, slug: `acme`, name: `Acme`, timeZone: `Europe/Vienna` }))
  return s
}

const CORS = { "access-control-allow-origin": `*`, "access-control-allow-headers": `*`, "access-control-allow-methods": `GET, POST, DELETE, OPTIONS`, "access-control-expose-headers": `mcp-session-id` }

Bun.serve({
  port: PORT,
  async fetch(request) {
    const url = new URL(request.url)
    if (request.method === `OPTIONS`) return new Response(null, { headers: CORS })
    if (url.pathname.startsWith(`/fonts/`)) {
      const file = Bun.file(`${WEB_PUBLIC}${url.pathname.slice(1)}`)
      return (await file.exists()) ? new Response(file, { headers: CORS }) : new Response(`not found`, { status: 404 })
    }
    if (url.pathname !== `/mcp`) return new Response(`not found`, { status: 404 })
    // Stateless, like /api/mcp: a fresh server + transport per request.
    const transport = new WebStandardStreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true })
    await server().connect(transport)
    const response = await transport.handleRequest(request)
    const headers = new Headers(response.headers)
    for (const [k, v] of Object.entries(CORS)) headers.set(k, v)
    return new Response(response.body, { status: response.status, headers })
  },
})
console.log(`connector harness: ${ORIGIN}/mcp`)
