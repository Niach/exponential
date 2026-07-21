import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  issueStatusValues,
  issuePriorityValues,
  issueSourceValues,
  teamRoleValues,
  boardIconValues,
  commentKindValues,
  notificationTypeValues,
  prStateValues,
  codingSessionStatusValues,
  subscriberSourceValues,
  issueEventTypeValues,
  issueStatusOrder,
  CODING_SESSION_STALE_HOURS,
} from "@exp/db-schema/domain"

// Guards that the hand-maintained TS enums in @exp/db-schema/domain stay in
// lockstep with the canonical packages/domain-contract/contract.json. If they
// drift, this test fails — contract.json is the single source of truth, and the
// generated native constants (Swift/Kotlin) come from the same file.
describe(`domain-contract parity`, () => {
  it(`issue status values + display order match the contract`, () => {
    expect([...issueStatusValues]).toEqual([...contract.issueStatus.values])
    expect([...issueStatusOrder]).toEqual([...contract.issueStatus.displayOrder])
  })

  it(`issue priority values match the contract`, () => {
    expect([...issuePriorityValues]).toEqual([...contract.issuePriority.values])
  })

  it(`issue source values match the contract`, () => {
    expect([...issueSourceValues]).toEqual([...contract.issueSource.values])
  })

  it(`team role values match the contract`, () => {
    expect([...teamRoleValues]).toEqual([...contract.teamRole.values])
  })

  it(`board icon values match the contract`, () => {
    expect([...boardIconValues]).toEqual([...contract.boardIcon.values])
  })

  it(`board icon values match the contract`, () => {
    expect([...boardIconValues]).toEqual([...contract.boardIcon.values])
  })

  it(`comment kind values match the contract`, () => {
    expect([...commentKindValues]).toEqual([...contract.commentKind.values])
  })

  it(`notification type values match the contract`, () => {
    expect([...notificationTypeValues]).toEqual([
      ...contract.notificationType.values,
    ])
  })

  it(`pr state values match the contract`, () => {
    expect([...prStateValues]).toEqual([...contract.prState.values])
  })

  it(`coding session status values match the contract`, () => {
    expect([...codingSessionStatusValues]).toEqual([
      ...contract.codingSessionStatus.values,
    ])
  })

  it(`coding session stale window matches the contract`, () => {
    expect(CODING_SESSION_STALE_HOURS).toBe(contract.codingSession.staleHours)
  })

  it(`subscriber source values match the contract`, () => {
    expect([...subscriberSourceValues]).toEqual([
      ...contract.subscriberSource.values,
    ])
  })

  it(`issue event type values match the contract`, () => {
    expect([...issueEventTypeValues]).toEqual([
      ...contract.issueEventType.values,
    ])
  })
})
