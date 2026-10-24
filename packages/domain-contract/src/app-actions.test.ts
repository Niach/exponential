import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-actions.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { APP_ACTIONS, actionConsent, actionGrantable, canGrantActions, declaredActions, grantAllows, parseActionName } from "./app-actions"

describe(`app actions (VAPP-120)`, () => {
  test(`every function is sorted, prefixed and has a display row`, () => {
    expect([...APP_ACTIONS]).toEqual([...APP_ACTIONS].sort())
    const rows = new Set(contract.expToolDisplay.tools.map((r) => r.name))
    for (const name of APP_ACTIONS) {
      expect(name.startsWith(fixture.prefix)).toBe(true)
      const t = parseActionName(name)!
      expect(rows.has(`${t.tool.replace(/^exponential_/, ``)}${t.op ? `_${t.op}` : ``}`), name).toBe(true)
    }
  })
  test.each(fixture.parseCases)(`parseActionName $name`, (c) => {
    expect(parseActionName(c.name)).toEqual(c.expected as never)
  })
  test.each(fixture.grantCases)(`grantAllows %j`, (c) => {
    expect(grantAllows(c.granted, c.name)).toBe(c.expected)
  })
  test.each(fixture.grantableCases)(`actionGrantable $name`, (c) => {
    expect(actionGrantable(c.name)).toBe(c.expected)
  })
  test.each(fixture.canGrantCases)(`canGrantActions $role`, (c) => {
    expect(canGrantActions(c.role)).toBe(c.expected)
  })
  test.each(fixture.declaredCases)(`declaredActions %j`, (c) => {
    expect(declaredActions(c.functions)).toEqual(c.expected)
  })
  test.each(fixture.consentCases)(`actionConsent $name`, (c) => {
    expect(actionConsent(c.name, c.args)).toEqual(c.expected as never)
  })
})
