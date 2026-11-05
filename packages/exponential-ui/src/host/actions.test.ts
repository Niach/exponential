// VAPP-194: fixtures/actions.json replayed through the TS reference (the
// Rust core, the FFI facade and the Swift / Compose SDKs replay the same
// file), plus the host's all-or-nothing commit over a MemoryRecordStore.

import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/actions.json"
import host from "../../catalog/host.json"
import { checkExpression, functionEffect, parseTimestamp } from "../cel"
import { ACTION_CALL_PREFIX, ACTION_LIMITS, ACTION_NAME_PATTERN, ACTION_STEP_KINDS, RECORDS_BATCH_FUNCTION, applyRecordBatch, runAction } from "./actions"
import type { ActionWrite } from "./actions"
import { validatePackage } from "./package"
import type { AppPackage } from "./package"
import type { RecordRow } from "./query"
import { MemoryRecordStore, recordsHost } from "./record-store"
import { ExponentialHost } from "./runtime"

const base = fixture.package as unknown as Record<string, unknown>
const now = parseTimestamp(fixture.env.now)!

/** A validate case's package: the base with its `with` keys replaced. */
export function validateCasePackage(c: { with: Record<string, unknown> }): Record<string, unknown> {
  const { addActions, ...rest } = c.with as { addActions?: Record<string, unknown> }
  return { ...base, ...rest, ...(addActions ? { actions: { ...(base.actions as Record<string, unknown>), ...addActions } } : {}) }
}

describe(`actions.json`, () => {
  test(`limits and the host contract`, () => {
    expect(fixture.limits).toEqual({ ...ACTION_LIMITS })
    expect(host.package.actions.limits).toEqual({ ...ACTION_LIMITS })
    expect(host.package.actions.stepKinds).toEqual([...ACTION_STEP_KINDS])
    expect(host.package.actions.namePattern).toBe(ACTION_NAME_PATTERN.source)
    expect(host.package.actions.callPrefix).toBe(ACTION_CALL_PREFIX)
    expect(host.package.actions.batchFunction).toBe(RECORDS_BATCH_FUNCTION)
  })

  for (const c of fixture.effects) test(`effect: ${c.name}`, () => expect(functionEffect(c.name)).toBe(c.effect as `pure`))

  for (const c of fixture.check as { name: string; expr: string; error?: string; type?: unknown }[])
    test(`check: ${c.name}`, () => expect(checkExpression(c.expr, {})).toEqual(c.error !== undefined ? { error: c.error } : ({ type: c.type } as never)))

  for (const c of fixture.validate as unknown as { name: string; with: Record<string, unknown>; issues: unknown[] }[])
    test(`validate: ${c.name}`, () => expect(validatePackage(validateCasePackage(c))).toEqual(c.issues as never))

  for (const c of fixture.run as unknown as { name: string; action: string; params?: unknown; data?: unknown; me?: string; maxSteps?: number; plan?: unknown; error?: string }[])
    test(`run: ${c.name}`, () => {
      const got = runAction(base, c.action, c.params, { data: c.data, me: c.me ?? null, now, offset: () => fixture.env.offset, maxSteps: c.maxSteps })
      expect(got).toEqual(c.error !== undefined ? { error: c.error } : ({ plan: c.plan } as never))
    })

  const b = fixture.batch
  for (const c of b.cases as unknown as { name: string; writes: ActionWrite[]; rows?: unknown; ids?: string[]; error?: string }[])
    test(`batch: ${c.name}`, () => {
      const got = applyRecordBatch(b.rows as RecordRow[], c.writes, { records: b.records, me: b.me, now: b.now, newId: (i) => b.newIds[i]! })
      expect(got).toEqual(c.error !== undefined ? { error: c.error } : ({ rows: c.rows, ids: c.ids } as never))
    })
})

describe(`a package action on a host`, () => {
  const surface = (opts: { failed?: string[]; toasts?: string[]; consent?: boolean } = {}) => {
    const pkg = base as unknown as AppPackage
    const store = new MemoryRecordStore({ me: `u1`, now: () => b.now, rows: b.rows as RecordRow[] })
    const records = recordsHost(pkg, store)
    const h = new ExponentialHost({
      packages: [pkg],
      sources: records.sources,
      functions: { ...records.functions, "harness.toast": (args) => void opts.toasts?.push(String(args.message)) },
      me: `u1`,
      now: () => now,
      zone: () => fixture.env.offset,
      onActionFailed: (f) => void opts.failed?.push(f.message),
      policy: { functions: { ask: [`harness.toast`] }, onFunctionCall: () => opts.consent ?? true },
    })
    h.receive({ version: `v0.9`, applyTemplate: { surfaceId: `s`, packageId: pkg.id, templateId: `month` } })
    let rows: readonly RecordRow[] = []
    store.subscribe((r) => (rows = r))
    return { h, store, rows: () => rows, data: () => h.surface(`s`)!.data }
  }
  const b = fixture.batch

  test(`the plan lands: one batch, the sets, then the calls`, async () => {
    const toasts: string[] = []
    const s = surface({ toasts })
    const out = await s.h.callFunction({ surfaceId: `s`, componentId: `log`, name: `actions.log`, args: { date: `2026-11-03`, hours: 2 } })
    expect(out.error).toBeUndefined()
    expect(s.rows().map((r) => r.data)).toContainEqual({ date: `2026-11-03`, hours: 2 })
    expect((s.data() as { form: { hours: number } }).form.hours).toBe(0)
    expect(toasts).toEqual([`Logged 2:00`])
  })

  test(`a false require lands nothing and is the failure's message`, async () => {
    const failed: string[] = []
    const s = surface({ failed })
    const before = s.rows()
    const out = await s.h.callFunction({ surfaceId: `s`, componentId: `log`, name: `actions.log`, args: { date: `2026-11-03`, hours: 0 } })
    expect(out.error).toBe(`Log more than 0 hours`)
    expect(failed).toEqual([`Log more than 0 hours`])
    expect(s.rows()).toBe(before)
    expect((s.data() as { form: { hours: number } }).form.hours).toBe(7.5)
  })

  test(`a refused write lands nothing: the batch is one unit`, async () => {
    const s = surface()
    const before = s.rows()
    const out = await s.h.callFunction({ surfaceId: `s`, componentId: `x`, name: `actions.swap`, args: { a: `e1`, b: `missing` } })
    expect(out.error).toBe(`write 1: record missing not found`)
    expect(s.rows()).toBe(before)
  })

  test(`a host call the person refuses lands nothing`, async () => {
    const s = surface({ consent: false })
    const before = s.rows()
    const out = await s.h.callFunction({ surfaceId: `s`, componentId: `log`, name: `actions.log`, args: { date: `2026-11-03`, hours: 2 } })
    expect(out.error).toBe(`harness.toast was not allowed`)
    expect(s.rows()).toBe(before)
  })

  // VAPP-205: a `call` step reaches the same built-ins a press does.
  for (const c of fixture.hostCalls.cases as unknown as { name: string; action: string; args: Record<string, unknown>; bound?: Record<string, string>; opened: string[]; error?: string }[])
    test(`hostCalls: ${c.name}`, async () => {
      const pkg = fixture.hostCalls.package as unknown as AppPackage
      const opened: string[] = []
      let bound: Record<string, string> | undefined
      const h = new ExponentialHost({
        packages: [pkg],
        sources: {
          records: (source, emit) => {
            bound = source.params
            emit([`bound`])
          },
        },
        policy: { openUrl: (url) => void opened.push(url) },
      })
      h.receive({ version: `v0.9`, applyTemplate: { surfaceId: `s`, packageId: pkg.id, templateId: `main` } })
      const out = await h.callFunction({ surfaceId: `s`, componentId: `b`, name: `actions.${c.action}`, args: c.args })
      expect(out.error).toBe(c.error as never)
      expect(opened).toEqual(c.opened)
      expect(bound).toEqual(c.bound)
      if (c.bound) expect((h.surface(`s`)!.data as { rows: unknown }).rows).toEqual([`bound`])
    })
})
