// eApps R3: fixtures/record-policy.json replayed through the TS reference.

import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/record-policy.json" with { type: "json" }
import { APP_ACCESS_LEVELS, DEFAULT_RECORD_POLICY, RECORD_POLICY_COPY, RECORD_POLICY_LEVELS, RECORD_POLICY_OPS, readableRows, recordPolicy, recordRefusal, validateRecordPolicy, type AppAccessLevel, type RecordOp } from "./record-policy"

describe(`record-policy.json`, () => {
  test(`the constants`, () => {
    expect(RECORD_POLICY_OPS).toEqual(fixture.ops as RecordOp[])
    expect(RECORD_POLICY_LEVELS).toEqual(fixture.levels as never)
    expect(APP_ACCESS_LEVELS).toEqual(fixture.access as never)
    expect(DEFAULT_RECORD_POLICY).toEqual(fixture.defaults as never)
    expect(RECORD_POLICY_COPY).toEqual(fixture.copy as never)
  })
  for (const c of fixture.validateCases) {
    test(`validate: ${c.name}`, () => {
      expect(validateRecordPolicy(c.policy, ``)).toEqual(c.issues)
      expect(recordPolicy({ policy: c.policy })).toEqual(c.effective as never)
    })
  }
  for (const c of fixture.allowCases as { name: string; policy?: unknown; op: RecordOp; viewer: string | null; role: string | null; access?: AppAccessLevel; author?: string | null; createdBy?: string | null; refusal: string | null }[]) {
    test(`allow: ${c.name}`, () => {
      const row = c.op === `create` ? undefined : { authorId: c.author, createdBy: c.createdBy }
      expect(recordRefusal(recordPolicy({ policy: c.policy }), c.op, { id: c.viewer, role: c.role, access: c.access }, row)).toBe(c.refusal)
    })
  }
  test(`readable: ${fixture.readable.name}`, () => {
    for (const v of fixture.readable.views as { viewer: string; role: string; access?: AppAccessLevel; ids: string[] }[]) {
      expect(readableRows(fixture.readable.rows, fixture.readable.kinds, { id: v.viewer, role: v.role, access: v.access }).map((r) => r.id)).toEqual(v.ids)
    }
  })
})
