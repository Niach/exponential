/**
 * The rich demo apps of the capture seed (VAPP-242/243 owed shots):
 * - CRM and Leave planner, the repo's signed samples (`samples/apps`), so
 *   RecordTable, RecordForm, Calendar and Timeline photograph real data;
 * - Ops desk, a small package declaring every server-side capability at once,
 *   so one Settings tab shows Accounts, MCP tools, Automations (a running one
 *   and a paused one), Webhooks, Public links, and its run view the Connect
 *   banner (the seeded `sentry` server has no credential for the demo user).
 */
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { db } from "@/db/connection"
import { apps, appShareLinks, mcpCalls } from "@/db/schema"
import { appPackageIssues } from "@/lib/apps/package"
import { publishVersionInTx, writeVersionInTx } from "@/lib/trpc/apps"
import { importRecords } from "@/lib/apps/records-service"
import { DEMO_OPS_APP_ID, DEMO_OPS_SHARE_TOKEN } from "../screenshot-demo"

const opsPackage = {
  formatVersion: 6,
  id: `exponential.ops-desk`,
  name: `Ops desk`,
  version: `1.0.0`,
  catalogId: `https://ui.exponential.at/catalogs/exponential-app/v1`,
  functions: [`records_*`],
  sources: [`records:lead`, `upstream:sentry/list_issues`],
  upstreams: [`sentry`, `stripe`],
  records: {
    lead: {
      title: `Lead`,
      policy: { read: `member`, create: `member`, update: `author`, delete: `author` },
      schema: {
        type: `object`,
        required: [`email`],
        properties: {
          email: { type: `string`, maxLength: 120, description: `Email` },
          note: { type: `string`, maxLength: 200, description: `What do you need?` },
        },
      },
    },
  },
  queries: { recent: { from: `lead`, sort: [[`createdAt`, `desc`]], limit: 20 } },
  exposes: [
    {
      name: `recent_leads`,
      title: `Recent leads`,
      description: `The newest leads.`,
      input: { type: `object`, properties: {} },
      access: `read`,
      run: { records: { op: `list`, kind: `lead`, query: `recent` } },
    },
    {
      name: `add_lead`,
      title: `Add a lead`,
      description: `Adds one lead.`,
      input: { type: `object`, properties: { email: { type: `string` } }, required: [`email`] },
      access: `write`,
      run: { records: { op: `upsert`, kind: `lead`, data: { email: `\${input.email}` } } },
    },
  ],
  automations: [
    { name: `log_deploy`, on: { webhook: { name: `deploys` } }, run: { records: { op: `upsert`, kind: `lead`, data: { email: `deploy@acme.example`, note: `\${event.version}` } } } },
    { name: `morning_digest`, on: { schedule: { cron: `0 8 * * 1-5`, tz: `Europe/Vienna` } }, sponsor: `gone@acme.example`, run: { records: { op: `upsert`, kind: `lead`, data: { email: `digest@acme.example`, note: `\${firedAt}` } } } },
  ],
  share: [
    { name: `signup`, mode: `submit`, kind: `lead`, fields: [`email`, `note`] },
    { name: `wall`, mode: `view`, kind: `lead`, fields: [`note`], query: `recent` },
  ],
  templates: {
    main: {
      bindings: [{ path: `/deploys`, source: `upstream:sentry/list_issues` }],
      data: { deploys: { rows: [] } },
      components: [
        { id: `root`, component: `Page`, title: `Ops desk`, children: [`table`] },
        { id: `table`, component: `RecordTable`, kind: `lead`, query: `sort=createdAt&order=desc&limit=50` },
      ],
    },
  },
}

/** The CRM sample ships companies and deals; Contacts (the `record-table` shot) needs rows. */
export const crmContacts = [
  { kind: `contact`, data: { name: `Ada Novak`, company: `Acme`, email: `ada@acme.example` } },
  { kind: `contact`, data: { name: `Ben Ortiz`, company: `Globex`, email: `ben@globex.example` } },
  { kind: `contact`, data: { name: `Chloe Lin`, company: `Initech`, email: `chloe@initech.example` } },
]

const leads = [
  { kind: `lead`, data: { email: `mira@northwind.example`, note: `Pricing for 40 seats` } },
  { kind: `lead`, data: { email: `tom@globex.example`, note: `Self-hosting questions` } },
]

/** The samples' dates sit in November 2026; move them to the current month so
 *  the Calendar and Timeline photograph a full month whenever the seed runs. */
function thisMonth(records: readonly { kind: string; data: Record<string, unknown> }[]): { kind: string; data: Record<string, unknown> }[] {
  const now = new Date()
  const months = (now.getUTCFullYear() - 2026) * 12 + now.getUTCMonth() - 10
  const shift = (value: unknown) => {
    if (typeof value !== `string` || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return value
    const d = new Date(`${value}T00:00:00Z`)
    d.setUTCMonth(d.getUTCMonth() + months)
    return d.toISOString().slice(0, 10)
  }
  return records.map((r) => ({ ...r, data: Object.fromEntries(Object.entries(r.data).map(([k, v]) => [k, shift(v)])) }))
}

async function installApp(
  teamId: string,
  userId: string,
  row: { id?: string; slug: string; name: string; icon: string | null; color: string | null; description: string },
  pkg: Record<string, unknown>,
  records: readonly { kind: string; data: Record<string, unknown> }[]
) {
  const issues = appPackageIssues(pkg)
  if (issues.length > 0) throw new Error(`demo app ${row.slug} is invalid: ${JSON.stringify(issues)}`)
  await db.transaction(async (tx) => {
    const [app] = await tx.insert(apps).values({ ...row, teamId, createdBy: userId }).returning()
    const { number } = await writeVersionInTx(tx, app!, pkg, { note: `Imported`, sessionId: null, createdBy: userId, byOwner: false })
    await publishVersionInTx(tx, app!, { number, byOwner: false, userId })
    await importRecords(tx, { ...app!, package: pkg as never }, userId, `owner`, records)
  })
}

export async function seedSampleApps(teamId: string, userId: string): Promise<void> {
  const samples = resolve(import.meta.dirname, `../../../../samples/apps`)
  for (const [slug, file] of [[`crm`, `crm`], [`leave-planner`, `leave-planner`]] as const) {
    const parsed = JSON.parse(readFileSync(`${samples}/${file}.app.json`, `utf8`))
    await installApp(teamId, userId, { slug, name: parsed.app.name, icon: parsed.app.icon, color: parsed.app.color, description: parsed.app.description }, parsed.package, [...thisMonth(parsed.records ?? []), ...(slug === `crm` ? crmContacts : [])])
  }

  await installApp(
    teamId,
    userId,
    { id: DEMO_OPS_APP_ID, slug: `ops-desk`, name: `Ops desk`, icon: `server`, color: `#ef4444`, description: `Deploys, leads and the morning digest.` },
    opsPackage,
    leads
  )
  await db.insert(appShareLinks).values({ appId: DEMO_OPS_APP_ID, teamId, name: `signup`, token: DEMO_OPS_SHARE_TOKEN, enabledBy: userId })
  await db.insert(mcpCalls).values({ teamId, userId, appId: DEMO_OPS_APP_ID, clientId: `automation:log_deploy`, tool: `log_deploy`, status: `ok`, latencyMs: 42, argsHash: `seed` })
}
