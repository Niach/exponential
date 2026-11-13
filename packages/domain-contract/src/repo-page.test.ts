// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/repo-page.json" with { type: "json" }
import { repoBranches, repoChecksRollup, repoPrTree, repoTab, type RepoPr } from "./repo-page"

describe(`repoPrTree`, () => {
  for (const entry of fixture.treeCases) {
    test(entry.name, () => expect(repoPrTree(entry.prs as RepoPr[])).toEqual(entry.expected as never))
  }
})

describe(`repoBranches`, () => {
  for (const entry of fixture.branchCases) {
    test(entry.name, () => expect(repoBranches(entry.branches, entry.defaultBranch, entry.prs as RepoPr[])).toEqual(entry.expected as never))
  }
})

describe(`repoChecksRollup`, () => {
  for (const entry of fixture.rollupCases) {
    test(entry.name, () => expect(repoChecksRollup(entry.prs as RepoPr[])).toBe(entry.expected))
  }
})

describe(`repoTab`, () => {
  for (const entry of fixture.tabCases) {
    test(String(entry.param), () => expect(repoTab(entry.param)).toBe(entry.expected as never))
  }
})
