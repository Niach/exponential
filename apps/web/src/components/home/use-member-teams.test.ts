import { describe, expect, it } from "vitest"
import type { Team } from "@/db/schema"
import { memberTeamsInOrder } from "@/components/home/use-member-teams"

const team = (id: string, name: string, kind?: string) => ({ id, name, slug: id, ...(kind ? { kind } : {}) }) as unknown as Team

describe(`#EXP-2027 memberTeamsInOrder (sidebar-tree.json teams)`, () => {
  it(`orders by name case-insensitively, ties by id, and never lists the personal team`, () => {
    const ordered = memberTeamsInOrder([
      team(`c`, `beta`),
      team(`b`, `Alpha`),
      team(`p`, `Me`, `personal`),
      team(`a`, `Alpha`),
    ])
    expect(ordered.map((row) => row.id)).toEqual([`a`, `b`, `c`])
  })
})
