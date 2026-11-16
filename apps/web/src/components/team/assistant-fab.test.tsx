import { describe, expect, it } from "vitest"
import { ASSISTANT_FAB, ASSISTANT_FAB_BUBBLE } from "@exp/domain-contract/agent-settings"
import type { TreeMessage } from "@exp/domain-contract/session-tree"
import { waitingCardView } from "./assistant-fab"

const message = (card: Record<string, unknown>, body = ``): TreeMessage => ({
  id: `m1`,
  sessionId: `s1`,
  authorKind: `agent`,
  authorId: null,
  body,
  card,
  answeredAt: null,
  consumedAt: null,
  createdAt: `2026-11-15T10:00:00Z`,
})

describe(`#EXP-1876 assistant FAB speech bubble`, () => {
  it(`a plan card: kicker plan, the chat list preview without its lead`, () => {
    expect(waitingCardView(message({ kind: `plan`, plan: `Bigger widget\n\nmore` }))).toEqual({
      cardKind: `plan`,
      text: `Bigger widget more`,
    })
  })

  it(`a question card: kicker question, the question`, () => {
    expect(waitingCardView(message({ kind: `question`, question: `Which base?` }))).toEqual({
      cardKind: `question`,
      text: `Which base?`,
    })
  })

  it(`the kicker icon is the needs-you concept (the web call site names it)`, () => {
    expect(ASSISTANT_FAB_BUBBLE.kickerIcon).toBe(`nav-needs-you`)
    expect(ASSISTANT_FAB.size).toBe(60)
  })
})
