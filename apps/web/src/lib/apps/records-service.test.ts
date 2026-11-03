import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import { describe, expect, it } from "vitest"

// #VAPP-193: every app-record read and write goes through the records
// service. Outside it, `appRecords` (the table) appears only in the admin
// paths that keep the owner role on purpose, and every entry point reaches
// the records through the service's functions.

const SRC = join(__dirname, `../..`)
const ALLOWED = new Set([
  `lib/apps/records-service.ts`, // the service
  `lib/app-records-purge.ts`, // the watchdog purge (owner role, hard delete)
  `lib/billing.ts`, // the plan quota count (team-wide, before the role switch)
  `lib/session-attachment-sweep.ts`, // VAPP-221: which app files records still reference (owner role)
  `routes/api/shapes/app-records.ts`, // Electric's shape proxy
])

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return name === `node_modules` ? [] : files(path)
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : []
  })
}

const read = (path: string) => readFileSync(join(SRC, path), `utf8`)

describe(`one records service`, () => {
  it(`no other server module touches app_records`, () => {
    const offenders = files(SRC)
      .map((path) => relative(SRC, path))
      .filter((path) => !ALLOWED.has(path))
      .filter((path) => /import \{[^}]*\bappRecords\b[^}]*\} from "@\/db\/schema"|\b(FROM|INTO|UPDATE|JOIN) app_records\b/.test(readFileSync(join(SRC, path), `utf8`)))
      // Client code reads the synced shape (collections), never the table.
      .filter((path) => !/^(components|routes\/(?!api)|lib\/collections)/.test(path) && !/\.tsx$/.test(path))
    expect(offenders).toEqual([])
  })

  it(`every entry point calls the service`, () => {
    const router = read(`lib/trpc/apps.ts`)
    for (const fn of [`listRecords(`, `queryRecords(`, `upsertRecord(`, `deleteRecord(`, `importRecords(`, `commitRecordBatch(`]) expect(router).toContain(fn)
    const broker = read(`lib/emcp/apps-broker.ts`)
    for (const fn of [`getAppRecord(`, `readAppRecordRows(`, `upsertRecord(`]) expect(broker).toContain(fn)
    // The MCP apps noun's records ops run the tRPC router (and so the service).
    const tools = read(`lib/emcp/tools.ts`)
    for (const op of [`apps.records.list(`, `apps.records.query(`, `apps.records.upsert(`, `apps.records.delete(`]) expect(tools).toContain(op)
  })

  it(`every records statement in the service runs under the role`, () => {
    const service = read(`lib/apps/records-service.ts`)
    // Each query over appRecords sits inside an asEapps/inRecordsScope callback.
    const scoped = (service.match(/(asEapps|inRecordsScope)\(/g) ?? []).length
    expect(scoped).toBeGreaterThanOrEqual(8)
    expect(service).toContain(`SET LOCAL ROLE`)
    expect(service).toMatch(/statement_timeout/)
  })
})
