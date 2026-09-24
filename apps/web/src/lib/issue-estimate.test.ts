import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-estimate.json"
import { ISSUE_ESTIMATE_TSHIRT_LABELS, ISSUE_ESTIMATION_SCALES } from "@/lib/domain"
import {
  estimateEventPhrase,
  estimateLabel,
  estimatePickerValues,
  estimateShortLabel,
  NO_ESTIMATE,
} from "@/lib/issue-estimate"

// EXP-630: the estimate helpers, locked ×4 against
// `domain-contract/fixtures/issue-estimate.json` — same cases, same test
// names on iOS (IssueEstimateTests), Android (IssueEstimateTest) and the
// desktop (domain::issue_estimate).

describe(`issue estimate (contract fixture)`, () => {
  it(`ladders and t-shirt labels match the fixture`, () => {
    expect(ISSUE_ESTIMATION_SCALES).toEqual(fixture.scales)
    expect([...ISSUE_ESTIMATE_TSHIRT_LABELS]).toEqual(fixture.tshirtLabels)
    expect(NO_ESTIMATE).toBe(fixture.noEstimate)
  })

  for (const row of fixture.labels) {
    it(`label: ${row.name}`, () => {
      expect(estimateLabel(row.value, row.scale)).toBe(row.label)
      if (row.value !== null) expect(estimateShortLabel(row.value, row.scale)).toBe(row.short)
    })
  }

  for (const row of fixture.pickers) {
    it(`picker: ${row.name}`, () => {
      expect(estimatePickerValues(row.current, row.scale)).toEqual(row.values)
    })
  }

  for (const row of fixture.phrases) {
    it(`phrase: ${row.name}`, () => {
      expect(estimateEventPhrase(row.payload, row.scale)).toBe(row.phrase)
    })
  }
})
