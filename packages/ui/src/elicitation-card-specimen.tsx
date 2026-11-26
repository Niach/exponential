import { useMemo, type ReactNode } from "react"
import fixture from "@exp/domain-contract/fixtures/elicitation-card.json"
import { chatAskRow, type SessionChatRow, type TreeMessage } from "@exp/domain-contract/session-tree"
import { ChatConversation } from "./session-chat-view"

// #NEW-146: THE elicitation card specimen (fixtures/elicitation-card.json
// ×4): the fixture's device-built cards through the REAL row model and
// `ChatConversation`: the waiting form (every field kind, defaults
// prefilled), the waiting url review (clean, then with its warnings) and an
// answered form. The app's `?dev=elicitation-card` capture renders it; the
// natives' specimens draw the same cards.

const card = (name: string) => fixture.cardCases.find((c) => c.name.startsWith(name))?.expected ?? null

const FORM = `a form with every field kind`
const URL = `a url: the domain`
const WARNED = `a url on http`

function row(id: string, cardValue: Record<string, unknown> | null, answeredAt: string | null = null): SessionChatRow[] {
  if (!cardValue) return []
  const message = {
    id,
    sessionId: `s-1`,
    card: cardValue,
    answeredAt,
  } as unknown as TreeMessage
  const built = chatAskRow(message)
  return built ? [built] : []
}

const text = (body: string): ReactNode => <p>{body}</p>
const noop = () => {}
const noSrc = () => ``

function Case({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      {children}
    </div>
  )
}

export function ElicitationCardSpecimen() {
  const rows = useMemo(() => {
    const form = card(FORM) as Record<string, unknown> | null
    const answered = form
      ? {
          ...form,
          answer: fixture.copy.submit,
          answers: [fixture.copy.submit],
          surfaceData: { form: { ...((form.surface as { data: { form: object } }).data.form), estimate: 3, due: `2026-12-01` } },
        }
      : null
    return {
      form: row(`e1`, form),
      url: row(`e2`, card(URL) as Record<string, unknown> | null),
      warned: row(`e3`, card(WARNED) as Record<string, unknown> | null),
      answered: row(`e4`, answered, `2026-11-26T12:00:00.000Z`),
    }
  }, [])
  return (
    <div data-testid="elicitation-card-specimen" className="flex max-w-2xl flex-col gap-6 text-sm">
      <Case label={fixture.copy.specimen.form}>
        <ChatConversation rows={rows.form} renderText={text} attachmentSrc={noSrc} onAnswer={noop} onDismiss={noop} />
      </Case>
      <Case label={fixture.copy.specimen.url}>
        <ChatConversation rows={rows.url} renderText={text} attachmentSrc={noSrc} onAnswer={noop} onDismiss={noop} />
      </Case>
      <Case label={fixture.copy.specimen.warned}>
        <ChatConversation rows={rows.warned} renderText={text} attachmentSrc={noSrc} onAnswer={noop} onDismiss={noop} />
      </Case>
      <Case label={fixture.copy.specimen.answered}>
        <ChatConversation rows={rows.answered} renderText={text} attachmentSrc={noSrc} />
      </Case>
    </div>
  )
}
