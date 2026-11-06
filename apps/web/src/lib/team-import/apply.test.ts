import { describe, expect, it } from "vitest"
import { TEAM_IMPORT_TABLES } from "@exp/domain-contract/team-import"
import { allTableMeta, tableMeta } from "@/lib/team-import/tables"
import { convertRow, IdMap, planRow, type ImportIdentity } from "@/lib/team-import/apply"

// #EXP-1713: the pure half of the commit over the real drizzle metadata (no
// database): the references the schema declares follow a remap, user
// columns take the identity rule, text columns lose the local handle,
// self references are deferred, a required column is named when missing.

const identity: ImportIdentity = { localUserId: `u-local`, localEmail: `danny@local.desktop`, callerId: `u-caller`, callerEmail: `danny@example.com` }

describe(`team import tables`, () => {
  it(`knows every fixture table and derives its references from the schema`, () => {
    expect(allTableMeta().map((m) => m.name)).toEqual([...TEAM_IMPORT_TABLES])
    const comments = tableMeta(`comments`)
    expect(comments.references).toMatchObject({ issue_id: `issues`, author_agent_id: `agents`, via_app_id: `apps` })
    expect(comments.selfReferences).toEqual([`parent_id`])
    expect(comments.userColumns).toEqual([`author_id`])
    expect(comments.teamColumns).toEqual([`team_id`])
    const sessions = tableMeta(`sessions`)
    // `parent_id` is named by sessions_root_check: it is written at insert (parents first).
    expect(sessions.references).toMatchObject({ root_id: `sessions`, parent_id: `sessions`, agent_id: `agents` })
    expect(sessions.selfReferences).toEqual([`continued_from_id`])
    expect(sessions.userColumns).toEqual([`user_id`])
    expect(tableMeta(`issue_labels`).hasId).toBe(false)
  })
})

describe(`planRow`, () => {
  it(`remaps references, applies the identity rule and the team`, () => {
    const ids = new IdMap()
    ids.set(`issues`, `i1`, `i1-new`)
    ids.set(`comments`, `c0`, `c0-new`)
    const planned = planRow(
      tableMeta(`comments`),
      { id: `c1`, issue_id: `i1`, team_id: `t-local`, board_id: `b1`, author_id: `u-other`, parent_id: `c0`, body: `hi @danny@local.desktop`, email_delivery_id: `e1`, unknown_column: 1, created_at: `2026-01-01T00:00:00.000Z` },
      `t-new`,
      identity,
      ids
    )
    expect(planned.row).toEqual({ id: `c1`, issue_id: `i1-new`, team_id: `t-new`, board_id: `b1`, author_id: null, parent_id: null, body: `hi @danny@example.com`, created_at: `2026-01-01T00:00:00.000Z` })
    expect(planned.deferred).toEqual({ parent_id: `c0-new` })
    expect(planned.remapped).toBe(1)
  })
  it(`a required user column becomes the caller`, () => {
    const planned = planRow(tableMeta(`sessions`), { id: `s1`, root_id: `s1`, user_id: `u-other`, device_id: `d1`, depth: 0 }, `t-new`, identity, new IdMap())
    expect(planned.row.user_id).toBe(`u-caller`)
    expect(planned.row.root_id).toBe(`s1`)
  })
  it(`arrays of ids follow, user arrays keep the caller`, () => {
    const ids = new IdMap()
    ids.set(`attachments`, `a2`, `a2-new`)
    const planned = planRow(
      tableMeta(`messages`),
      { id: `m1`, conversation_id: `cv1`, root_id: `s1`, author_kind: `user`, author_id: `u-local`, attachment_ids: [`a1`, `a2`], mention_user_ids: [`u-local`, `u-other`], ref_issue_ids: [`i1`], body: `` },
      `t-new`,
      identity,
      ids
    )
    expect(planned.row).toMatchObject({ author_id: `u-caller`, attachment_ids: [`a1`, `a2-new`], mention_user_ids: [`u-caller`], ref_issue_ids: [`i1`] })
    expect(planned.remapped).toBe(1)
  })
  it(`drops what stays local`, () => {
    const planned = planRow(tableMeta(`boards`), { id: `b1`, name: `B`, slug: `b`, prefix: `B`, repository_id: `r1`, default_branch: `main` }, `t-new`, identity, new IdMap())
    expect(planned.row).toEqual({ id: `b1`, name: `B`, slug: `b`, prefix: `B`, team_id: `t-new` })
  })
})

describe(`convertRow`, () => {
  it(`keys by column, parses timestamps and names a missing required column`, () => {
    const meta = tableMeta(`labels`)
    expect(convertRow(meta, { id: `l1`, team_id: `t`, name: `Bug`, created_at: `2026-01-01T00:00:00.000Z`, color: null }, `l1`)).toEqual({ id: `l1`, teamId: `t`, name: `Bug`, createdAt: new Date(`2026-01-01T00:00:00.000Z`) })
    expect(() => convertRow(meta, { id: `l1`, team_id: `t` }, `l1`)).toThrow(`labels l1: name is required`)
    expect(() => convertRow(meta, { id: `l1`, team_id: `t`, name: `x`, created_at: `nope` }, `l1`)).toThrow(`created_at is not a timestamp`)
  })
})
