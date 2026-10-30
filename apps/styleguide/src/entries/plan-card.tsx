import { ExponentialUiTwin, PlanCardSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1280: THE rich plan / question card (fixture `plan-card.json` ×4):
// `PlanCardSpecimen`, session-thread.json's rich case through the REAL
// `ThreadConversation` (the app's `?dev=plan-card` capture renders the same
// board).

export const entry: StyleguideEntry = {
  id: `plan-card`,
  section: `special`,
  owner: `EXP-1280`,
  title: `Plan card`,
  blurb: `Plan mode is Exponential's: a session in plan mode posts ONE rich card (\`exponential_plan_propose\`) and stops; \`exponential_sessions_ask\` posts the same kind of card as one question. The card = the AskCard header + the markdown (pictures inline; an \`upload:\` picture renders nothing until it lands) + the agent's Exponential UI surface in its \`Preview\` frame (the real renderer, no transport, \`set\` only) + one block per question: the prompt, Choose one / Choose any, then THE option rows (text only) or picture tiles (\`ChoiceTiles\`, once any option has a picture) and the \`Other…\` free-text row. A pending plan with two or more questions steps through them one at a time ("Question 1 of 2", Back / Next). N-12: a pending card the viewer may decide waits in THE Dock and opens as its sheet (ask-dock entry); the transcript keeps the card read-only. The last page's pinned footer carries THE decision: "No, keep planning" (secondary; picked = the feedback field above the footer, Cancel + the reject as the primary) and "Yes" (the primary; its menu holds "Yes, and start with a fresh context"); approving waits on every question ("Answer every question to approve"), rejecting never does. A question card's footer = Answer. Answered = closed: the picks highlighted, the rest inert, the decision label and the feedback. Model = \`answerIssue\`, \`optionRows\`, \`pictureFor\` (fixture \`plan-card.json\` ×4). Exponential UI twin (VAPP-162): Plan, the APPROVAL CHECKLIST an agent puts in a surface (a computer-use plan): a card with the title, the description and the steps as trace lines (pending · running · done · skipped · error), proposed = Decline + Approve, running = a progress bar and Stop, done or declined = no footer; painted below from specimens.json \`exponential-ui-plan\`. It is a surface piece, not this card: the card stays the session's plan mode.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadPlanCard`,
      file: `packages/ui/src/plan-card.tsx`,
      note: `cards: session-thread-view.tsx; session view (tool-posted cards): agent-session.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `plan_parts`,
      file: `apps/desktop/crates/ui/src/plan_card.rs`,
      note: `styleguide/entries/plan_card.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `PlanCardQuestionBlock`,
      file: `apps/ios/Exponential/UI/Thread/PlanCardForm.swift`,
      note: `model: ExpCore PlanCard.swift`,
    },
    android: {
      state: `ok`,
      symbol: `PlanAskCard`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/PlanCardView.kt`,
      note: `model: domain/PlanCard.kt`,
    },
  },
  island: () => (
    <ExponentialUiTwin name="Plan" id="exponential-ui-plan">
      <PlanCardSpecimen />
    </ExponentialUiTwin>
  ),
}
