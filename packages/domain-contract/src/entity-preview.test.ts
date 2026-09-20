import { describe, expect, it } from "bun:test"
import {
  CHIP_LABEL_MAX,
  ENTITY_REF_ICON,
  ENTITY_REF_KINDS,
  entityChipDetail,
  entityChipLabel,
  entityKindNoun,
  entityRefIcon,
  groupPreviewRefs,
  type EntityRefLike,
} from "./entity-preview"
import fixture from "../fixtures/entity-chip.json" with { type: "json" }
import icons from "../../icons/icons.json" with { type: "json" }

// EXP-920: the entity-preview chip rule, locked ×4 (desktop
// domain::entity_preview, iOS EntityPreviewTests, Android EntityPreviewTest)
// against the ONE contract fixture — same cases, same test names.

interface ChipCase {
  name: string
  ref: EntityRefLike
  label: string
  detail: string | null
  icon: string
}

interface GroupCase {
  name: string
  refs: EntityRefLike[]
  groups: { ref: number; members: number[] }[]
}

const chips = fixture.chips as ChipCase[]
const groups = fixture.groups as GroupCase[]

describe(`entity preview`, () => {
  it(`every chip case renders byte-exact`, () => {
    expect(chips.length).toBeGreaterThanOrEqual(24)
    for (const { name, ref, label, detail, icon } of chips) {
      expect(entityChipLabel(ref), name).toBe(label)
      expect(entityChipDetail(ref), name).toBe(detail)
      expect(entityRefIcon(ref), name).toBe(icon)
    }
  })

  it(`every group case groups the same`, () => {
    expect(groups.length).toBeGreaterThanOrEqual(6)
    for (const { name, refs, groups: expected } of groups) {
      const actual = groupPreviewRefs(refs).map((group) => ({
        ref: refs.indexOf(group.ref),
        members: group.members.map((member) => refs.indexOf(member)),
      }))
      expect(actual, name).toEqual(expected)
    }
  })

  it(`every contract kind has an icon concept that exists, and a noun`, () => {
    const semantic = (icons as { semantic: Record<string, string> }).semantic
    for (const kind of ENTITY_REF_KINDS) {
      expect(ENTITY_REF_ICON[kind], kind).toBeDefined()
      expect(semantic[ENTITY_REF_ICON[kind]!], kind).toBeDefined()
      expect(entityKindNoun(kind, 1), kind).not.toBe(`item`)
      expect(entityKindNoun(kind, 2), kind).not.toBe(`items`)
    }
    expect(Object.keys(ENTITY_REF_ICON).sort()).toEqual([...ENTITY_REF_KINDS].sort())
  })

  it(`the fixture covers every kind's chip`, () => {
    const seen = new Set(chips.map((c) => (c.ref.kind === `list` ? `list` : c.ref.kind)))
    for (const kind of ENTITY_REF_KINDS) expect(seen.has(kind), kind).toBe(true)
  })

  it(`a label never exceeds the cap`, () => {
    for (const { ref } of chips) {
      expect(Array.from(entityChipLabel(ref)).length).toBeLessThanOrEqual(CHIP_LABEL_MAX)
    }
  })
})
