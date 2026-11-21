import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sidebar-compat.json" with { type: "json" }
import type { SidebarEntry } from "./sidebar-folders"
import { mergeKindIntoTeam, projectTeamToKind, type SidebarCompatKind, type SidebarTargetKind } from "./sidebar-compat"

const kindOf = (id: string) => ((fixture.kinds as Record<string, string>)[id] ?? null) as SidebarTargetKind | null
const rows = (list: unknown) => list as SidebarEntry[]
const mint = (pattern: string, key: string) => (id: string) => pattern.replace(`{${key}}`, id)

describe(`sidebar compat (#EXP-2027)`, () => {
  for (const c of fixture.project) {
    test(`project: ${c.name}`, () => {
      expect(
        projectTeamToKind({
          team: rows(fixture.team),
          kindOf,
          kind: c.kind as SidebarCompatKind,
          existing: rows(c.existing),
          mint: mint(fixture.mintProject, `teamFolderId`),
        })
      ).toEqual(c.expected as never)
    })
  }
  for (const c of fixture.merge) {
    test(`merge: ${c.name}`, () => {
      expect(
        mergeKindIntoTeam({
          team: rows((c as { team?: unknown }).team ?? fixture.team),
          kindRows: rows(c.kindRows),
          kind: c.kind as SidebarCompatKind,
          kindOf,
          mint: mint(fixture.mintMerge, `kindFolderId`),
        })
      ).toEqual(c.expected as never)
    })
  }
})
