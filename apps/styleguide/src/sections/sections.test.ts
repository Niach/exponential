import { existsSync } from "node:fs"
import path from "node:path"

import { describe, expect, test } from "bun:test"

import { COMPONENTS } from "../components.tsx"
import { ENTRIES, entryById } from "../entries/index.ts"
import { ENTRY_OWNERS, SECTIONS, SECTION_ENTRY_IDS, sectionOf } from "./index.ts"

// EXP-1029 contract — the section skeleton. Leaves fill their entry file;
// this keeps the index honest until EXP-1019 wires the page to it.

describe(`the four sections`, () => {
  test(`are numbered 1..4 in order, with the agreed titles`, () => {
    expect(SECTIONS.map((section) => section.order)).toEqual([1, 2, 3, 4])
    expect(SECTIONS.map((section) => section.title)).toEqual([
      `1 Style`,
      `2 General components`,
      `3 Special components`,
      `4 Views`,
    ])
    expect(SECTIONS.map((section) => section.id)).toEqual([`style`, `general`, `special`, `views`])
  })

  test(`every registered id is unique and collides with no existing component id`, () => {
    expect(new Set(SECTION_ENTRY_IDS).size).toBe(SECTION_ENTRY_IDS.length)
    const existing = new Set(COMPONENTS.map((spec) => spec.id))
    for (const id of SECTION_ENTRY_IDS) expect(existing.has(id) ? `collides: ${id}` : id).toBe(id)
  })

  test(`every registered id has an owner and a placeholder file`, () => {
    for (const id of SECTION_ENTRY_IDS) {
      expect(ENTRY_OWNERS[id]).toMatch(/^EXP-\d+$/)
      expect(existsSync(path.resolve(import.meta.dir, `..`, `entries`, `${id}.tsx`))).toBe(true)
    }
  })
})

describe(`the entries`, () => {
  test(`match the index one to one, in its order`, () => {
    expect(ENTRIES.map((entry) => entry.id)).toEqual([...SECTION_ENTRY_IDS])
  })

  test(`each sits in the section that lists it and names its owner`, () => {
    for (const entry of ENTRIES) {
      expect(sectionOf(entry.id)?.id).toBe(entry.section)
      expect(entry.owner).toBe(ENTRY_OWNERS[entry.id]!)
      expect(entryById(entry.id)).toBe(entry)
    }
  })

  test(`a placeholder renders one line naming its owner; a filled entry has a demo and a status table`, () => {
    for (const entry of ENTRIES) {
      if (entry.placeholder) {
        expect(entry.render?.()).toContain(entry.owner)
        expect(entry.island).toBeUndefined()
      } else {
        expect(entry.render !== undefined || entry.island !== undefined).toBe(true)
        expect(entry.status).toBeDefined()
      }
    }
  })
})
