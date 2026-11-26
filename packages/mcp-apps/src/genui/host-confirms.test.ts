// #NEW-149: where the host declared form elicitation, the SERVER confirms a
// view's delete or stop through the host (elicitation.json D6), so the
// genUI view skips its own in-view confirm for exactly those calls.
import { hostConfirmsCall } from "./mcp-host"
import { decodeGenuiResult, HOST_CONFIRMS_META } from "./result"

const ID = `0b8f6c2e-3a51-4c07-9d1e-6f1b2a3c4d5e`

describe(`hostConfirmsCall`, () => {
  it(`an action that deletes one id is the server's to confirm`, () => {
    expect(hostConfirmsCall(`exponential_issues_delete`, { id: `EXP-12` })).toBe(true)
    expect(hostConfirmsCall(`exponential_comments_delete`, { id: ID })).toBe(true)
  })
  it(`harness_mcp: a destructive op or a stop on the write tools`, () => {
    expect(hostConfirmsCall(`harness_mcp`, { tool: `exponential_issues_write`, arguments: { op: `delete`, id: `EXP-12` } })).toBe(true)
    expect(hostConfirmsCall(`harness_mcp`, { tool: `exponential_sessions_send`, arguments: { sessionId: ID, stop: true } })).toBe(true)
  })
  it(`anything else keeps the in-view confirm`, () => {
    expect(hostConfirmsCall(`exponential_issues_create`, { title: `x` })).toBe(false)
    expect(hostConfirmsCall(`harness_mcp`, { tool: `exponential_issues_write`, arguments: { op: `update`, id: `EXP-12` } })).toBe(false)
    expect(hostConfirmsCall(`harness_mcp`, { tool: `exponential_sessions_send`, arguments: { sessionId: ID, text: `hi` } })).toBe(false)
    // No single id = the server refuses it without asking: the view asks.
    expect(hostConfirmsCall(`exponential_issues_delete`, { id: `EXP-1, EXP-2` })).toBe(false)
    expect(hostConfirmsCall(`harness_toast`, {})).toBe(false)
  })
})

describe(`the server's mark`, () => {
  const app = { id: ID, slug: `standup`, name: `Standup`, teamId: ID, package: {} }
  it(`an app read with the mark runs with hostConfirms`, () => {
    expect(decodeGenuiResult({ content: [{ type: `text`, text: JSON.stringify(app) }], _meta: { [HOST_CONFIRMS_META]: true } })).toEqual({ kind: `app`, app, hostConfirms: true })
  })
  it(`without it, the view confirms as before`, () => {
    expect(decodeGenuiResult({ content: [{ type: `text`, text: JSON.stringify(app) }] })).toEqual({ kind: `app`, app })
  })
})
