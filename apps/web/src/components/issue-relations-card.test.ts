import { describe, expect, it } from "vitest"
import {
  RELATION_SIDES,
  pickLabel,
  rowLabel,
} from "@/components/issue-relations-card"
import {
  groupRelationRows,
  relationGroupTitle,
  relationLabel,
} from "@/lib/issue-relations"

// EXP-736 — relation wording is a CROSS-CLIENT contract with two halves, and
// the web is the client that once conflated them:
//
//   * the PICKER entry names an action ("Blocking"), and is a literal, pinned
//     table — iOS `RelationPick.all`, Android `RELATION_PICKS` and the desktop
//     picker carry the same six strings in the same order;
//   * the ROW caption states the stored fact and is the contract label
//     verbatim, lowercase ("blocked by") — iOS renders `Text(relation.label)`,
//     Android `"${relation.label} · IDENT"`.
//
// Deriving the first from the second is what produced the web-only "Blocks".

const NATIVE_PICKS: Array<[string, string]> = [
  [`parent:forward`, `Parent of`],
  [`parent:inverse`, `Sub-issue of`],
  [`blocks:forward`, `Blocking`],
  [`blocks:inverse`, `Blocked by`],
  [`duplicate:forward`, `Duplicate of`],
  [`related:forward`, `Related to`],
]

describe(`the "Add relation" picker`, () => {
  it(`offers the six native picks, in the native order`, () => {
    expect(
      RELATION_SIDES.filter((entry) => entry.pickable).map((entry) => [
        entry.side,
        entry.pickLabel,
      ])
    ).toEqual(NATIVE_PICKS)
  })

  it(`labels every pickable side and no other`, () => {
    for (const entry of RELATION_SIDES) {
      expect(entry.pickLabel === null).toBe(!entry.pickable)
    }
    // `duplicated by` and the mirrored `related` side are never offered: the
    // first belongs on the OTHER issue, the second is the same row read back.
    expect(
      RELATION_SIDES.filter((entry) => !entry.pickable).map((e) => e.side)
    ).toEqual([`duplicate:inverse`, `related:inverse`])
  })

  it(`does not capitalize the contract label into the menu`, () => {
    // The regression this pins: "Blocks" (and any other auto-capitalized
    // label) must not reach the menu.
    expect(pickLabel(`blocks`, `forward`)).toBe(`Blocking`)
    expect(pickLabel(`blocks`, `forward`)).not.toBe(`Blocks`)
    expect(pickLabel(`parent`, `inverse`)).toBe(`Sub-issue of`)
  })
})

describe(`a relation row's caption`, () => {
  it(`is the contract label verbatim, as the natives render it`, () => {
    expect(rowLabel(`parent`, `forward`)).toBe(`parent of`)
    expect(rowLabel(`parent`, `inverse`)).toBe(`sub-issue of`)
    expect(rowLabel(`blocks`, `forward`)).toBe(`blocks`)
    expect(rowLabel(`blocks`, `inverse`)).toBe(`blocked by`)
    expect(rowLabel(`duplicate`, `forward`)).toBe(`duplicate of`)
    expect(rowLabel(`duplicate`, `inverse`)).toBe(`duplicated by`)
    expect(rowLabel(`related`, `forward`)).toBe(`related to`)
  })

  it(`never diverges from the contract table`, () => {
    for (const entry of RELATION_SIDES) {
      expect(rowLabel(entry.type, entry.direction)).toBe(
        relationLabel(entry.type, entry.direction)
      )
    }
  })
})

// EXP-760 — the card is gone: rows render under the shared heading groups
// (lib/issue-relations.ts, mirrored by desktop `group_title`). The table this
// file owns is the one that feeds them, so the pairing is pinned here: every
// side the picker offers must have a heading to land under, or a relation the
// user just created would render into nothing.
describe(`every relation side reaches a heading group`, () => {
  it(`titles all eight sides`, () => {
    for (const entry of RELATION_SIDES) {
      const title = relationGroupTitle(entry.type, entry.direction)
      expect(title.length).toBeGreaterThan(0)
    }
  })

  it(`groups a row from each side, in the heading order`, () => {
    const rows = RELATION_SIDES.map((entry, index) => ({
      id: index,
      type: entry.type,
      direction: entry.direction,
    }))
    const groups = groupRelationRows(rows)
    // Both `related` sides merge into one heading, so eight sides make seven
    // groups — and no row is ever dropped.
    expect(groups.map((group) => group.title)).toEqual([
      `Sub-issues`,
      `Parent`,
      `Blocked by`,
      `Blocks`,
      `Duplicate of`,
      `Duplicated by`,
      `Related`,
    ])
    expect(groups.flatMap((group) => group.rows)).toHaveLength(
      RELATION_SIDES.length
    )
  })
})
