import { useState, type ReactNode } from "react"
import {
  ELICITATION_CARD_COPY,
  ELICITATION_CARD_ICONS,
  elicitationMissing,
} from "@exp/domain-contract/elicitation-card"
import { askDockTitle } from "@exp/domain-contract/ask-dock"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import fixture from "@exp/domain-contract/fixtures/elicitation-card.json"
import { AskAnsweredLine, AskCard, AskDismissedCard, AskPrompt } from "./ask-card"
import { Button } from "./button"
import { conceptIcon } from "./icons.generated"
import { PlanCardSurface } from "./plan-card"

const ServerIcon = conceptIcon(ELICITATION_CARD_ICONS.server as never)
const TEST_IDS = fixture.testIds

type QuestionRow = Extract<SessionChatRow, { type: `question` }>

/** #NEW-146 (elicitation-card.json): what an answer carries back. */
export type ElicitationAnswer = {
  answer: string
  answers: string[]
  surfaceData?: Record<string, unknown>
}

/** No prefetch, never an in-app view: the URL opens on the press only, in
 *  a new tab that cannot reach back (`noopener,noreferrer`). */
function openElicitationUrl(url: string) {
  window.open(url, `_blank`, `noopener,noreferrer`)
}

/** THE elicitation card ×4 (elicitation-card.json RENDER): the question
 *  card's chrome with the asking SERVER as its label, the message, the
 *  device-built surface (the form, or the URL review) and ONE end-aligned
 *  button row. Answered = closed: the surface inert, the pick named. */
export function ElicitationCard({
  row,
  renderText,
  busy,
  onAnswer,
  onDismiss,
}: {
  row: QuestionRow & { elicitation: NonNullable<QuestionRow[`elicitation`]> }
  renderText: (text: string) => ReactNode
  busy: boolean
  onAnswer?: (input: ElicitationAnswer) => void
  onDismiss?: () => void
}) {
  const elicitation = row.elicitation
  const [data, setData] = useState<Record<string, unknown> | null>(row.surface?.data ?? null)
  if (row.dismissed) {
    return <AskDismissedCard title={askDockTitle(row.question)} data-card-id={row.id} />
  }
  const interactive = !row.answered && !busy && onAnswer !== undefined
  const missing = elicitation.mode === `form` ? elicitationMissing(elicitation, data) : []
  const send = (label: string, withData: boolean) => {
    if (!interactive) return
    onAnswer?.({ answer: label, answers: [label], ...(withData && data ? { surfaceData: data } : {}) })
  }
  const copy = ELICITATION_CARD_COPY
  return (
    <AskCard
      label={
        <span data-testid={TEST_IDS.server} className="inline-flex min-w-0 items-center gap-1">
          <ServerIcon className="size-3 shrink-0" />
          <span className="truncate">{elicitation.server}</span>
        </span>
      }
      onDismiss={!row.answered && !busy ? onDismiss : undefined}
      data-slot="chat-question"
      data-testid={TEST_IDS.card}
      data-kind="question"
      data-card-id={row.id}
      data-answered={row.answered || undefined}
    >
      {row.question !== `` && <div className="text-sm text-foreground/90">{renderText(row.question)}</div>}
      {row.surface && (
        <PlanCardSurface
          surface={row.surface}
          data={row.answered ? row.surfaceData : null}
          disabled={!interactive}
          testKey={row.id}
          caption={false}
          onData={setData}
        />
      )}
      {row.answered ? (
        <AskAnsweredLine answer={row.answers?.[0] ?? row.answer} />
      ) : busy ? (
        <AskPrompt options={[]} answerable={false} locked={{ labels: [] }} onAnswer={() => {}} />
      ) : onAnswer ? (
        <div className="mt-3 flex flex-col items-end gap-1">
          <div className="flex flex-wrap justify-end gap-2">
            {elicitation.mode === `url` ? (
              <>
                <Button type="button" size="sm" variant="outline" data-testid={TEST_IDS.decline} onClick={() => send(copy.decline, false)}>
                  {copy.decline}
                </Button>
                <Button type="button" size="sm" variant="outline" data-testid={TEST_IDS.done} onClick={() => send(copy.done, false)}>
                  {copy.done}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  data-testid={TEST_IDS.openLink}
                  onClick={() => elicitation.url && openElicitationUrl(elicitation.url)}
                >
                  {copy.openLink}
                </Button>
              </>
            ) : (
              <>
                <Button type="button" size="sm" variant="ghost" data-testid={TEST_IDS.cancel} onClick={() => send(copy.cancel, false)}>
                  {copy.cancel}
                </Button>
                <Button type="button" size="sm" variant="outline" data-testid={TEST_IDS.decline} onClick={() => send(copy.decline, false)}>
                  {copy.decline}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  data-testid={TEST_IDS.submit}
                  disabled={missing.length > 0}
                  onClick={() => send(copy.submit, true)}
                >
                  {copy.submit}
                </Button>
              </>
            )}
          </div>
          {missing.length > 0 && <p className="text-[0.6875rem] text-yellow-400">{copy.missing}</p>}
        </div>
      ) : null}
    </AskCard>
  )
}
