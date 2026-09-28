import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/coding-readiness.json"
import {
  codingReadiness,
  readinessAgo,
  READINESS_COPY,
  readinessDeviceBody,
  readinessLastSeen,
  readinessPickerUsedBy,
  readinessRepositoryBody,
  readinessRepositoryTitle,
  readinessSummary,
  type CodingReadinessInput,
} from "@/lib/coding-readiness"

// EXP-1121: the readiness model + its copy, fixture-locked ×4 — desktop
// `domain::coding_readiness`, iOS `CodingReadiness.swift` and Android
// `CodingReadiness.kt` run the SAME json. Change one, change all four.

describe(`coding-readiness (EXP-1121)`, () => {
  it(`locks the copy table`, () => {
    const derived: Record<string, string> = {
      ...READINESS_COPY,
      "repositoryTitle(App)": readinessRepositoryTitle(`App`),
      "repositoryBody(App)": readinessRepositoryBody(`App`),
      "deviceBody(Acme)": readinessDeviceBody(`Acme`),
      "lastSeen(MacBook Pro, 2 h ago)": readinessLastSeen(`MacBook Pro`, `2 h ago`),
      "pickerUsedBy(Website)": readinessPickerUsedBy(`Website`),
      "summary(0,3)": readinessSummary(0, 3),
      "summary(2,3)": readinessSummary(2, 3),
      "summary(3,3)": readinessSummary(3, 3),
    }
    expect(derived).toEqual(fixture.copy)
  })

  it.each(fixture.ago)(`ago $expected`, ({ nowMs, thenMs, expected }) => {
    expect(readinessAgo(nowMs, thenMs)).toBe(expected)
  })

  it.each(fixture.cases)(`$name`, ({ input, expected }) => {
    expect(codingReadiness(input as CodingReadinessInput)).toEqual(expected)
  })
})
