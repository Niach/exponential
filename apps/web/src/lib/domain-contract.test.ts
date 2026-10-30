import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import { PLAN_CARD_CAPS } from "@exp/domain-contract/plan-card"
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
  deviceIconValues,
  commentKindValues,
  commentSourceValues,
  commentAudienceValues,
  notificationTypeValues,
  prStateValues,
  DEVICE_AGENT_DEFAULTS,
  sessionKindValues,
  sessionStatusValues,
  sessionIssueSourceValues,
  messageAuthorKindValues,
  messageCardKindValues,
  messageEventKindValues,
  SESSIONS_SEND_TEXT_MAX,
  SESSIONS_SEND_MAX_ATTACHMENTS,
  SESSIONS_SEND_MAX_ISSUES,
  SESSION_BRIEF_MAX_BYTES,
  MESSAGE_BODY_MAX,
  AGENT_NAME_MAX,
  AGENT_INSTRUCTIONS_MAX_BYTES,
  SESSION_TREE_MAX_DEPTH,
  SESSION_TREE_MAX_SESSIONS,
  codingSessionBlockedKindValues,
  codingSessionBlockedWindowValues,
  subscriberSourceValues,
  pinKindValues,
  issueEventTypeValues,
  issueRelationTypeValues,
  issueRelationSourceValues,
  ISSUE_RELATION_LABELS,
  issueStatusOrder,
  CODING_SESSION_STALE_HOURS,
  mcpAuthValues,
  mcpTransportValues,
  MAX_START_PROMPT,
  MAX_START_PROMPT_IMAGES,
  agentTriggerKindValues,
  builtinEventNames,
  eventEmitterValues,
  CRON_MAX,
  TZ_MAX,
  MAX_AGENT_TRIGGERS,
  EVENT_NAME_MAX,
  EVENT_PAYLOAD_MAX_BYTES,
  PLAN_CARD_DESCRIPTION_MAX,
  PLAN_CARD_LABEL_KEY_MAX,
  PLAN_CARD_LABEL_MAX,
  PLAN_CARD_OPTIONS_MAX,
  PLAN_CARD_PICTURES_MAX,
  PLAN_CARD_PROMPT_MAX,
  PLAN_CARD_QUESTIONS_MAX,
  PLAN_CARD_SURFACE_MAX_BYTES,
  PLAN_CARD_SURFACE_MAX_COMPONENTS,
  PLAN_CARD_UPLOAD_SCHEME,
} from "@exp/db-schema/domain"
import { MAX_STEER_IMAGES } from "@/lib/steer-image-message"
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

  it(`device icon values match the contract`, () => {
    expect([...deviceIconValues]).toEqual([...contract.deviceIcon.values])
  })

  it(`comment kind values match the contract`, () => {
    expect([...commentKindValues]).toEqual([...contract.commentKind.values])
  })

  it(`comment source values match the contract (EXP-741)`, () => {
    expect([...commentSourceValues]).toEqual([
      ...contract.commentSource.values,
    ])
  })

  it(`comment audience values match the contract (SLOP-4)`, () => {
    expect([...commentAudienceValues]).toEqual([
      ...contract.commentAudience.values,
    ])
  })

  it(`notification type values match the contract`, () => {
    expect([...notificationTypeValues]).toEqual([
      ...contract.notificationType.values,
    ])
  })

  it(`pr state values match the contract`, () => {
    expect([...prStateValues]).toEqual([...contract.prState.values])
  })

  it(`device agent defaults match the contract`, () => {
    expect(DEVICE_AGENT_DEFAULTS).toEqual({
      model: contract.deviceAgentDefaults.model,
      subagentModel: contract.deviceAgentDefaults.subagentModel,
      threadModel: contract.deviceAgentDefaults.threadModel,
    })
    // Every model is explicit (owner 2026-10-15): no blank default.
    expect(DEVICE_AGENT_DEFAULTS).toEqual({
      model: `fable`,
      subagentModel: `opus`,
      threadModel: `haiku`,
    })
  })

  // EXP-1291: the session tree's vocabulary + sessions.send's caps.
  it(`session tree vocabulary and caps match the contract`, () => {
    expect([...sessionKindValues]).toEqual([...contract.sessionKind.values])
    expect([...sessionStatusValues]).toEqual([...contract.sessionStatus.values])
    expect([...sessionIssueSourceValues]).toEqual([
      ...contract.sessionIssueSource.values,
    ])
    expect([...messageAuthorKindValues]).toEqual([
      ...contract.messageAuthorKind.values,
    ])
    expect([...messageCardKindValues]).toEqual([
      ...contract.messageCardKind.values,
    ])
    expect([...messageEventKindValues]).toEqual([
      ...contract.messageEventKind.values,
    ])
    // S-3: the agent reuse thresholds (`reuseMax*`) are server-only.
    expect(contract.sessionsSend).toMatchObject({
      textMax: SESSIONS_SEND_TEXT_MAX,
      maxAttachments: SESSIONS_SEND_MAX_ATTACHMENTS,
      maxIssues: SESSIONS_SEND_MAX_ISSUES,
      briefMaxBytes: SESSION_BRIEF_MAX_BYTES,
      messageBodyMax: MESSAGE_BODY_MAX,
      agentNameMax: AGENT_NAME_MAX,
      agentInstructionsMaxBytes: AGENT_INSTRUCTIONS_MAX_BYTES,
      treeMaxDepth: SESSION_TREE_MAX_DEPTH,
      treeMaxSessions: SESSION_TREE_MAX_SESSIONS,
    })
  })

  // EXP-1280: the plan / question card caps (exponential_plan_propose,
  // exponential_sessions_ask) are hand-mirrored into db-schema/domain.ts.
  it(`plan card caps match the contract`, () => {
    // `contract`'s generated type predates the section: read the JSON.
    const { $comment: _comment, ...caps } = PLAN_CARD_CAPS
    expect(caps).toEqual({
      questionsMax: PLAN_CARD_QUESTIONS_MAX,
      optionsMax: PLAN_CARD_OPTIONS_MAX,
      picturesMax: PLAN_CARD_PICTURES_MAX,
      promptMax: PLAN_CARD_PROMPT_MAX,
      labelMax: PLAN_CARD_LABEL_MAX,
      descriptionMax: PLAN_CARD_DESCRIPTION_MAX,
      labelKeyMax: PLAN_CARD_LABEL_KEY_MAX,
      surfaceMaxBytes: PLAN_CARD_SURFACE_MAX_BYTES,
      surfaceMaxComponents: PLAN_CARD_SURFACE_MAX_COMPONENTS,
      uploadScheme: PLAN_CARD_UPLOAD_SCHEME,
    })
  })

  // EXP-804: the usage-wall row state. Both vocabularies are hand-mirrored
  // into db-schema/domain.ts, so the contract owns them like every enum here.
  it(`coding session blocked vocabulary matches the contract`, () => {
    expect([...codingSessionBlockedKindValues]).toEqual([
      ...contract.codingSessionBlocked.kinds,
    ])
    expect([...codingSessionBlockedWindowValues]).toEqual([
      ...contract.codingSessionBlocked.windows,
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

  it(`pin kind values match the contract`, () => {
    expect([...pinKindValues]).toEqual([...contract.pinKind.values])
  })

  it(`issue event type values match the contract`, () => {
    expect([...issueEventTypeValues]).toEqual([
      ...contract.issueEventType.values,
    ])
  })

  // EXP-736: the relation vocabulary AND both label halves — every client
  // renders its side's label from the generated contract, so a reordered
  // values list or a reworded label would silently desync four UIs.
  it(`issue relation types, sources and labels match the contract`, () => {
    expect([...issueRelationTypeValues]).toEqual([
      ...contract.issueRelationType.values,
    ])
    expect([...issueRelationSourceValues]).toEqual([
      ...contract.issueRelationSource.values,
    ])
    expect(
      issueRelationTypeValues.map((type) => ISSUE_RELATION_LABELS[type].forward)
    ).toEqual([...contract.issueRelationType.forwardLabels])
    expect(
      issueRelationTypeValues.map((type) => ISSUE_RELATION_LABELS[type].inverse)
    ).toEqual([...contract.issueRelationType.inverseLabels])
  })

  it(`mcp server transport + auth match the contract (EXP-792)`, () => {
    expect([...mcpTransportValues]).toEqual([...contract.mcpTransport.values])
    expect([...mcpAuthValues]).toEqual([...contract.mcpAuth.values])
  })

  it(`start prompt limits match the contract`, () => {
    // EXP-825: the start's free text + its image embeds.
    expect(MAX_START_PROMPT).toBe(contract.startPrompt.maxLength)
    expect(MAX_START_PROMPT_IMAGES).toBe(contract.startPrompt.maxImages)
    expect(MAX_STEER_IMAGES).toBe(contract.startPrompt.maxImages)
  })

  // NEW-2: triggers live on the `agents` definition (schedule | event) and
  // the server fires them; events are the team's stream.
  it(`agent trigger kinds + caps match the contract (NEW-2)`, () => {
    expect([...agentTriggerKindValues]).toEqual([...contract.agentTrigger.kindValues])
    expect(MAX_AGENT_TRIGGERS).toBe(contract.agentTrigger.maxPerAgent)
    expect(CRON_MAX).toBe(contract.agentTrigger.cronMax)
    expect(TZ_MAX).toBe(contract.agentTrigger.tzMax)
  })

  it(`event names, emitters + caps match the contract (NEW-2)`, () => {
    expect([...builtinEventNames]).toEqual([...contract.events.builtinNames])
    expect([...eventEmitterValues]).toEqual([...contract.events.emitterValues])
    expect(EVENT_NAME_MAX).toBe(contract.events.nameMax)
    expect(EVENT_PAYLOAD_MAX_BYTES).toBe(contract.events.payloadMaxBytes)
  })
})

// EXP-724: the curated steer slash-command catalog — the one list every
// viewer's `/` menu offers and the desktop publisher executes.
describe(`steer command catalog`, () => {
  const commands = contract.steerCommands.commands

  it(`names are unique slash-safe tokens with non-empty descriptions`, () => {
    const names = commands.map((c) => c.name)
    expect(new Set(names).size).toBe(names.length)
    for (const command of commands) {
      expect(command.name).toMatch(/^[a-z][a-z0-9-]*$/)
      expect(command.description.trim().length).toBeGreaterThan(0)
      expect(typeof command.argHint).toBe(`string`)
      expect(typeof command.confirm).toBe(`boolean`)
    }
  })

  it(`every command names at least one known coding agent`, () => {
    for (const command of commands) {
      expect(command.agents.length).toBeGreaterThan(0)
      for (const agent of command.agents) {
        expect(contract.codingAgent.values).toContain(agent)
      }
    }
  })

  // NEW-2 (owner 2026-10-21): the builtins are compact, model and effort,
  // on every agent, none confirm-gated; `/clear` and `/btw` are gone.
  // Thread 58cb32df: `/new` = a fresh conversation in the same session.
  it(`is exactly compact, new, model and effort, for every agent`, () => {
    expect(commands.map((c) => c.name)).toEqual([`compact`, `new`, `model`, `effort`])
    for (const command of commands) {
      expect(command.agents).toEqual(contract.codingAgent.values)
      expect(command.confirm).toBe(false)
    }
  })
})
