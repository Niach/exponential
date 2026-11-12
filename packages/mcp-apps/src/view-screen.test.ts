import { nounCall } from "./tool-names"
import { defaultDisplayMode, screenForCall } from "./view-screen"

const text = (data: unknown) => ({ content: [{ type: `text`, text: JSON.stringify(data) }] })
const issue = { id: `i1`, identifier: `EXP-1`, title: `One`, status: `backlog`, priority: `high` }

describe(`nounCall (#VAPP-292)`, () => {
  it(`routes a per-operation name to its noun + op`, () => {
    expect(nounCall(`exponential_issues_get`, { id: `EXP-1` })).toEqual({ name: `exponential_issues`, args: { op: `get`, id: `EXP-1` } })
    expect(nounCall(`exponential_members_list`, { teamId: `t` })).toEqual({ name: `exponential_people`, args: { op: `list`, teamId: `t` } })
    expect(nounCall(`exponential_devices_account_login`, { deviceId: `d` }).args.op).toBe(`account_login`)
  })
  it(`turns the retired session verbs into sessions_send`, () => {
    expect(nounCall(`exponential_sessions_start`, { deviceId: `d`, issueId: `EXP-1`, agent: `codex` })).toEqual({
      name: `exponential_sessions_send`,
      args: { deviceId: `d`, issueIds: [`EXP-1`], cli: `codex` },
    })
  })
  it(`passes a noun or verb as is`, () => {
    expect(nounCall(`exponential_apps`, { op: `get` })).toEqual({ name: `exponential_apps`, args: { op: `get` } })
    expect(nounCall(`exponential_show`, {}).name).toBe(`exponential_show`)
  })
})

describe(`screenForCall (#VAPP-292)`, () => {
  it(`draws a list from structuredContent with its page`, () => {
    const result = { content: [{ type: `text`, text: `1 issue` }], structuredContent: { items: [issue], page: { offset: 0, limit: 50, nextOffset: null } } }
    expect(screenForCall(`issues`, `show`, result)).toEqual({ kind: `list`, rows: [issue], page: { offset: 0, limit: 50, nextOffset: null } })
  })
  it(`still reads an older server's JSON text`, () => {
    expect(screenForCall(`issues`, `list`, text([issue]))).toEqual({ kind: `list`, rows: [issue], page: null })
  })
  it(`draws the issue a get answered and reloads one a write answered`, () => {
    expect(screenForCall(`issues`, `get`, text(issue))).toEqual({ kind: `issue`, id: `i1`, detail: issue })
    expect(screenForCall(`issues`, `create`, text(issue))).toEqual({ kind: `issue`, id: `i1`, detail: null })
    expect(screenForCall(`issues`, `update_status`, text({ issue }))).toEqual({ kind: `issue`, id: `i1`, detail: null })
  })
  it(`draws nothing for an op that answers no list or issue`, () => {
    for (const op of [`delete`, `labels_add`, `subscribe`, `activity`]) {
      expect(screenForCall(`issues`, op, text({ ok: true }))).toEqual({ kind: `none` })
    }
    expect(screenForCall(`inbox`, `mark_read`, text({ ok: true }))).toEqual({ kind: `none` })
    expect(screenForCall(`devices`, `account_login`, text({ status: `url` }))).toEqual({ kind: `none` })
    // A failed write is the chat's to tell.
    expect(screenForCall(`issues`, `update`, { isError: true, content: [{ type: `text`, text: `nope` }] })).toEqual({ kind: `none` })
  })
  it(`opens a board the person asked to see full size`, () => {
    expect(defaultDisplayMode(`issues`, `show`)).toBe(`fullscreen`)
    expect(defaultDisplayMode(`issues`, `list`)).toBe(`inline`)
    expect(defaultDisplayMode(`devices`, `list`)).toBe(`inline`)
  })
})
