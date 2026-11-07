import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-actions.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { APP_ACTIONS, actionConsent, actionGrantable, actionOwnerOnly, canGrantActions, declaredActions, flatAction, grantAllows, parseActionName, storedAction } from "./app-actions"

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
  test(`storedAction (round 9 §14) = the dotted name a released client reads`, () => {
    expect(storedAction(`exponential_issues_update_status`)).toBe(`exponential.issues.update_status`)
    expect(storedAction(`exponential_mcp_servers_list`)).toBe(`exponential.mcp_servers.list`)
    expect(storedAction(`exponential_sessions_send`)).toBe(`exponential.sessions.send`)
    expect(storedAction(`exponential.issues.get`)).toBe(`exponential.issues.get`)
    for (const name of APP_ACTIONS) expect(flatAction(storedAction(name))).toBe(name)
  })
  test(`aliases (EXP-1657) are never offered and each names an offered action`, () => {
    for (const [old, name] of Object.entries(fixture.aliases)) {
      expect(APP_ACTIONS.includes(old), old).toBe(false)
      expect(APP_ACTIONS.includes(name), name).toBe(true)
      expect(parseActionName(old)).toEqual(parseActionName(name))
    }
  })
  test.each(fixture.parseCases)(`parseActionName $name`, (c) => {
    expect(parseActionName(c.name)).toEqual(c.expected as never)
  })
  test.each(fixture.grantCases)(`grantAllows %j`, (c) => {
    expect(grantAllows(c.granted, c.name, (c as { args?: Record<string, unknown> }).args ?? {})).toBe(c.expected)
  })
  test.each(fixture.grantableCases)(`actionGrantable $name`, (c) => {
    expect(actionGrantable(c.name)).toBe(c.expected)
  })
  test.each(fixture.ownerOnlyCases)(`actionOwnerOnly $name`, (c) => {
    expect(actionOwnerOnly(c.name)).toBe(c.expected)
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
