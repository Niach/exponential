import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  issueStatusValues,
  issueStatusCategoryValues,
  issueStatusCategoryDisplayOrder,
  ISSUE_STATUS_STARTED_MAX,
  CATEGORY_ANCHOR,
  BUILTIN_STATUS_DEFAULTS,
  issuePriorityValues,
  issueSourceValues,
  teamRoleValues,
  boardIconValues,
  commentKindValues,
  notificationTypeValues,
  prStateValues,
  codingSessionStatusValues,
  codingSessionEndedByValues,
  subscriberSourceValues,
  issueEventTypeValues,
  issueStatusOrder,
  CODING_SESSION_STALE_HOURS,
  actionInputTypeValues,
  MAX_ACTION_INPUTS,
  MAX_ACTION_INPUT_TEXT,
  actionTriggerEventValues,
  actionScheduleIntervalValues,
  MAX_TRIGGER_FILTER_IDS,
} from "@exp/db-schema/domain"
import {
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_FIX_CONFLICTS_ID,
} from "@/lib/builtin-actions"
import {
  getIssuePriorityConfig,
  getIssueStatusConfig,
  issuePriorityOptions,
  issueStatusOptions,
} from "@/lib/domain"

// Guards that the hand-maintained TS enums in @exp/db-schema/domain stay in
// lockstep with the canonical packages/domain-contract/contract.json. If they
// drift, this test fails — contract.json is the single source of truth, and the
// generated native constants (Swift/Kotlin) come from the same file.
describe(`domain-contract parity`, () => {
  it(`issue status values + display order match the contract`, () => {
    expect([...issueStatusValues]).toEqual([...contract.issueStatus.values])
    expect([...issueStatusOrder]).toEqual([...contract.issueStatus.displayOrder])
  })

  // EXP-314: status categories + the locked builtin defaults.
  it(`issue status category values + order + cap match the contract`, () => {
    expect([...issueStatusCategoryValues]).toEqual([
      ...contract.issueStatusCategory.values,
    ])
    // EXP-448: ONE category order — settings sections, set-status pickers and
    // issue-list groups all walk it, so the lists read exactly as the settings
    // page lays them out.
    expect([...issueStatusCategoryDisplayOrder]).toEqual([
      `backlog`,
      `unstarted`,
      `started`,
      `completed`,
      `cancelled`,
      `duplicate`,
    ])
    expect([...issueStatusCategoryDisplayOrder]).toEqual([
      ...contract.issueStatusCategory.displayOrder,
    ])
    expect(ISSUE_STATUS_STARTED_MAX).toBe(
      contract.issueStatusCategory.startedMax
    )
  })

  it(`builtin status defaults match the contract`, () => {
    expect(BUILTIN_STATUS_DEFAULTS).toEqual([...contract.issueStatusDefaults])
    // Exactly one default per builtin enum value, and the anchor of every
    // default's category resolves back into the enum.
    expect(BUILTIN_STATUS_DEFAULTS.map((d) => d.key)).toEqual([
      ...issueStatusValues,
    ])
    for (const d of BUILTIN_STATUS_DEFAULTS) {
      expect(issueStatusCategoryValues).toContain(d.category)
      expect(issueStatusValues).toContain(CATEGORY_ANCHOR[d.category])
    }
  })

  // The SQL seed trigger must transcribe the contract defaults verbatim —
  // it's the only writer of builtin rows for NEW teams, and every client's
  // fallback set assumes these exact values.
  it(`the seed trigger's builtin rows mirror the contract defaults`, () => {
    const triggersSql = readFileSync(
      join(__dirname, `../db/out/custom/0001_triggers.sql`),
      `utf8`
    )
    for (const d of BUILTIN_STATUS_DEFAULTS) {
      expect(triggersSql).toContain(
        `(NEW.id, '${d.category}', '${d.name}', '${d.color}', ${d.sortOrder}, '${d.key}')`
      )
    }
  })

  it(`issue priority values match the contract`, () => {
    expect([...issuePriorityValues]).toEqual([...contract.issuePriority.values])
  })

  // REV2-85: pickers speak ONE order on every client — the contract display
  // order. The web option tables ARE that order (they back every status /
  // priority menu, the filter popover and the bulk bar).
  it(`option tables are ordered by the contract display order`, () => {
    expect(issueStatusOptions.map((option) => option.value)).toEqual([
      ...contract.issueStatus.displayOrder,
    ])
    expect(issuePriorityOptions.map((option) => option.value)).toEqual([
      ...contract.issuePriority.displayOrder,
    ])
  })

  // Unknown/forward-compat wire values must still resolve to the lifecycle
  // start, not to whatever now sits first in the display-ordered table.
  it(`unknown values fall back to backlog / no priority`, () => {
    expect(getIssueStatusConfig(`triaged`).value).toBe(`backlog`)
    expect(getIssuePriorityConfig(`blocker`).value).toBe(`none`)
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

  // EXP-637: who ended a run (EXP-686 dropped the self-reported outcome).
  it(`coding session endedBy values match the contract`, () => {
    expect([...codingSessionEndedByValues]).toEqual([
      ...contract.codingSessionEndedBy.values,
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

  it(`action input types + limits + builtin id match the contract (EXP-257)`, () => {
    expect([...actionInputTypeValues]).toEqual([
      ...contract.actionInputType.values,
    ])
    expect(MAX_ACTION_INPUTS).toBe(contract.actionInputs.max)
    expect(MAX_ACTION_INPUT_TEXT).toBe(contract.actionInputs.maxTextLength)
    expect(BUILTIN_CREATE_ACTION_ID).toBe(contract.builtinAction.createActionId)
    expect(BUILTIN_FIX_CONFLICTS_ID).toBe(
      contract.builtinAction.fixConflictsId
    )
  })

  it(`action trigger vocabulary + filter cap match the contract (EXP-530)`, () => {
    expect([...actionTriggerEventValues]).toEqual([
      ...contract.actionTrigger.eventValues,
    ])
    expect([...actionScheduleIntervalValues]).toEqual([
      ...contract.actionTrigger.scheduleIntervalValues,
    ])
    expect(MAX_TRIGGER_FILTER_IDS).toBe(contract.actionTrigger.maxFilterIds)
    // Trigger events are strictly a subset of the issue-event vocabulary the
    // devices watch (`created` etc. included above).
    for (const event of contract.actionTrigger.eventValues) {
      expect(issueEventTypeValues).toContain(event)
    }
  })
})
