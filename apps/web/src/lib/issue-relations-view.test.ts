import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-relations-view.json"
import {
  issueRelationsView,
  RELATIONS_BAND_CAP,
  RELATIONS_VIEW_COPY,
  relationsProgress,
  relationsShowMore,
  type RelationsViewInput,
} from "@/lib/issue-relations-view"

// EXP-1097: the relations view model + its copy, fixture-locked ×4 — desktop
// `domain::relations_view`, iOS `IssueRelationsView.swift` and Android
// `IssueRelationsView.kt` run the SAME json. Change one, change all four.

describe(`issue-relations-view (EXP-1097)`, () => {
  it(`locks the copy table`, () => {
    expect({
      ...RELATIONS_VIEW_COPY,
      "showMore(4)": relationsShowMore(4),
      "progress(2,5)": relationsProgress(2, 5),
      cap: String(RELATIONS_BAND_CAP),
    }).toEqual(fixture.copy)
  })

  it.each(fixture.cases)(`$name`, ({ input, expected }) => {
    expect(issueRelationsView(input as RelationsViewInput)).toEqual(expected)
  })
})
