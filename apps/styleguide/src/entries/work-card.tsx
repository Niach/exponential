import {
  workCard,
  workDurationLong,
  workDurationShort,
  workModelLabel,
  type WorkCardInput,
  type WorkCardWorkflowAgent,
} from "@exp/domain-contract/work-card"
import fixture from "@exp/domain-contract/fixtures/work-card.json"
import { WorkCard, type WorkCardFoldedPhase } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1240 (EXP-1259 D12): THE Work card — the REAL `WorkCard` painting the
// contract model (`workCard()`) over the fixture's own cases, the same three
// the IDE entry draws: a workflow mid-review, a task list in progress, a
// finished subagent.

const CASES = [0, 5, 9].map((index) => fixture.cases[index].input as WorkCardInput)

/** The workflow's folded phases spelled out, the way the app's
 *  `workflowFoldedPhases` does, so the folded row carries its chevron. */
function foldedPhases(input: WorkCardInput): WorkCardFoldedPhase[] | undefined {
  if (input.kind !== `workflow`) return undefined
  const card = workCard(input)
  if (card.rows[0]?.type !== `folded`) return undefined
  const count = card.segments.length - (card.rows.length - 1)
  const agents: readonly WorkCardWorkflowAgent[] = input.agents ?? []
  return [...(input.phases ?? [])]
    .sort((a, b) => a.index - b.index)
    .slice(0, count)
    .map((phase) => {
      const lanes = agents.filter((agent) => agent.phaseIndex === phase.index)
      const span = Math.max(0, ...lanes.map((agent) => agent.durationMs ?? 0))
      return {
        title: phase.title,
        duration: span > 0 ? workDurationShort(span) : null,
        lanes: lanes.map((agent) => ({
          label: agent.label ?? `Agent ${agent.index}`,
          model: workModelLabel(agent.model),
          state: agent.state,
          duration:
            typeof agent.durationMs === `number` ? workDurationLong(agent.durationMs) : null,
        })),
      }
    })
}

export const entry: StyleguideEntry = {
  id: `work-card`,
  section: `special`,
  owner: `EXP-1240`,
  title: `Work card`,
  blurb: `ONE card for a claude workflow, the agent's task list and a subagent, replacing the workflow card, the task list line and the subagent group rows. Header: the kind icon, the name, the muted kind word, then trailing the progress (\`16 / 17\`, \`3 / 7\`, \`127 calls\`) and the elapsed time, a 12px spinner in front while it runs. A segment bar under it (one capsule per phase or task, never for a subagent). Finished leading phases fold into ONE row (\`Contract · Implement · Conformance · 5 lanes · 48m ›\`, the chevron opens their lanes in place); the running phase lists its lanes one line each (dot, name, model, duration or spinner); a pending phase is a bare title. A lane row and a subagent card open that agent's transcript tab; a task row does nothing. No report text, no token counts, no JSON. Model = \`workCard()\` (fixture \`work-card.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `WorkCard`,
      file: `packages/ui/src/work-card.tsx`,
      note: `model: @exp/domain-contract/work-card; inputs: apps/web/src/lib/agent-feed.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `work_card`,
      file: `apps/desktop/crates/ui/src/work_card.rs`,
      note: `styleguide/entries/work_card.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `WorkCardView`,
      file: `apps/ios/ExpUI/Sources/WorkCardView.swift`,
      note: `model: ExpCore WorkCard.swift`,
    },
    android: {
      state: `ok`,
      symbol: `WorkCard`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/WorkCard.kt`,
      note: `model: domain/WorkCard.kt`,
    },
  },
  island: () => (
    <div className="flex max-w-2xl flex-col gap-3 text-sm">
      {CASES.map((input, index) => (
        <WorkCard
          key={index}
          card={workCard(input)}
          foldedPhases={foldedPhases(input)}
          onOpenLane={() => {}}
          onOpenSubagent={input.kind === `subagent` ? () => {} : undefined}
        />
      ))}
    </div>
  ),
}
