// #EXP-2067 (chat-express.json): the web transcript draws an agent text's
// Express blocks in place, over the committed wasm.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ExpressDecoder, initExpress, unwrapExpress } from "@exponential_app/eui/express"
import { EXPRESS_COPY, syncedRowKey } from "@exp/domain-contract/chat-express"
import type { CallView, SessionChatRow } from "@exp/domain-contract/session-tree"
import { CallViewBody } from "./call-view"
import { ExpressMessage, provideExpressSdk } from "./express-message"
import { ChatConversation, type ChatConversationProps } from "./session-chat-view"
import { TooltipProvider } from "./tooltip"
import { SessionCard } from "./work-card"

initExpress(readFileSync(resolve(__dirname, `../../exponential-ui/wasm/express/a2ui_express_bg.wasm`)))
provideExpressSdk({ unwrap: (text) => unwrapExpress(text), decoder: (surfaceId) => new ExpressDecoder({ surfaceId, wrapped: false }) })

const BLOCK = `Here is the build:\n<a2ui>\nroot = Stack(children=[title, ship])\ntitle = Text(text="Build 1482 is green")\nship = Button(label="Ship it", on={press: Event("reply", userMessage="Ship it")})\n</a2ui>\nShip it?`

type MessageRow = Extract<SessionChatRow, { type: `message` }>

const agentRow = (body: string): MessageRow => ({
  type: `message`,
  id: `m1`,
  author: `agent`,
  authorId: null,
  body,
  queued: false,
  authorSessionId: null,
  xref: null,
})

const prose = (text: string, part: { first: boolean; last: boolean; key?: string }) => (
  <p key={part.key ?? `p`} data-testid="prose" data-first={part.first} data-last={part.last}>
    {text}
  </p>
)

describe(`ExpressMessage`, () => {
  it(`no block: today's bubble`, () => {
    render(<ExpressMessage rowKey="m:m1" text="Merged #212." final messageId="m1" renderProse={prose} fallback={<p data-testid="fallback">x</p>} />)
    expect(screen.getByTestId(`fallback`)).toBeTruthy()
  })

  it(`splits prose, the block's surface in THE surface card frame, prose; the time on the last prose`, () => {
    render(<ExpressMessage rowKey="m:m1" text={BLOCK} final messageId="m1" renderProse={prose} fallback={null} />)
    const runs = screen.getAllByTestId(`prose`)
    expect(runs.map((r) => r.textContent)).toEqual([`Here is the build:`, `Ship it?`])
    expect(runs.map((r) => [r.dataset.first, r.dataset.last])).toEqual([
      [`true`, `false`],
      [`false`, `true`],
    ])
    expect(screen.getByTestId(`express-block`).dataset.status).toBe(`ok`)
    expect(screen.getByTestId(`surface-card`).textContent).toContain(`Build 1482 is green`)
  })

  it(`a press posts the event with its block origin`, () => {
    const onAction = vi.fn()
    render(<ExpressMessage rowKey="m:m1" text={BLOCK} final messageId="m1" renderProse={prose} fallback={null} onAction={onAction} />)
    act(() => {
      fireEvent.click(screen.getByRole(`button`, { name: `Ship it` }))
    })
    expect(onAction).toHaveBeenCalledTimes(1)
    const [event, origin] = onAction.mock.calls[0]!
    expect(event).toMatchObject({ name: `reply`, userMessage: `Ship it` })
    expect(origin).toEqual({ surfaceId: `m:m1#1`, block: `m:m1#1`, messageId: `m1` })
  })

  it(`a failed block draws THE marker`, () => {
    render(
      <ExpressMessage rowKey="m:m1" text={`<a2ui>\nroot = Gauge(value=3)\n</a2ui>`} final messageId="m1" renderProse={prose} fallback={null} />
    )
    expect(screen.getByTestId(`express-block`).dataset.status).toBe(`failed`)
    expect(screen.getByTestId(`transcript-marker`).textContent).toContain(EXPRESS_COPY.blockFailed)
  })

  it(`a streaming row holds back a tag still arriving`, () => {
    render(<ExpressMessage rowKey="n:s1:x" text={`Checking the deploy.\n<a2`} final={false} messageId={null} renderProse={prose} fallback={null} />)
    expect(screen.getAllByTestId(`prose`).map((r) => r.textContent)).toEqual([`Checking the deploy.`])
  })
})

function Conversation(props: Partial<ChatConversationProps> & { rows: SessionChatRow[] }) {
  return (
    <TooltipProvider>
      <ChatConversation renderText={(text) => <p>{text}</p>} attachmentSrc={(id) => `/api/attachments/${id}`} {...props} />
    </TooltipProvider>
  )
}

describe(`ChatConversation`, () => {
  it(`an agent row draws its blocks; an action press reaches onSurfaceAction`, () => {
    const onSurfaceAction = vi.fn()
    render(<Conversation rows={[agentRow(BLOCK)]} onSurfaceAction={onSurfaceAction} />)
    expect(screen.getByTestId(`surface-card`).textContent).toContain(`Build 1482 is green`)
    act(() => {
      fireEvent.click(screen.getByRole(`button`, { name: `Ship it` }))
    })
    expect(onSurfaceAction.mock.calls[0]?.[1]).toEqual({ surfaceId: `m:m1#1`, block: syncedRowKey(`m1`) + `#1`, messageId: `m1` })
  })

  it(`a person's pasted tag stays text`, () => {
    render(<Conversation rows={[{ ...agentRow(BLOCK), author: `user`, authorId: `u1` }]} viewerId="u1" />)
    expect(screen.queryByTestId(`surface-card`)).toBeNull()
  })

  it(`an action row without a body is THE marker`, () => {
    render(
      <Conversation
        rows={[{ ...agentRow(``), author: `user`, authorId: `u1`, action: { name: `save_deal` } }]}
        viewerId="u1"
      />
    )
    expect(screen.getByTestId(`transcript-marker`).textContent).toContain(`You pressed save_deal`)
  })
})

// #EXP-2076: THE call view's spoken turns are the chat's rows: an agent turn
// with a block splits like the chat and its action posts into the call
// session; the summary band and a call card's caption only READ the text.
describe(`CallViewBody`, () => {
  const SAMPLE = `Checked the three lanes.\n<a2ui>\nroot = Stack(children=[title, ship])\ntitle = Text(text="Build 1482 is green")\nship = Button(label="Ship it", on={press: Event("reply", userMessage="Ship it")})\n</a2ui>\nAll green.`
  const view = (summary: string | null): CallView => ({
    sessionId: `call1`,
    title: `Call · 4 min`,
    subtitle: `Today 09:08 – 09:12 · Assistant`,
    running: false,
    summary,
    summaryLabel: `Summary`,
    rows: [{ ...agentRow(SAMPLE), id: `v7` }],
  })
  const body = (summary: string | null, onSurfaceAction?: ChatConversationProps[`onSurfaceAction`]) =>
    render(
      <TooltipProvider>
        <CallViewBody
          view={view(summary)}
          heading
          renderText={(text) => <p data-testid="text">{text}</p>}
          attachmentSrc={() => ``}
          chat={{ sessionId: `r1`, title: `Assistant`, agentId: null }}
          onSurfaceAction={onSurfaceAction}
        />
      </TooltipProvider>
    )

  it(`an agent turn draws its block between its prose bubbles; a press posts with the turn's origin`, () => {
    const onSurfaceAction = vi.fn()
    body(null, onSurfaceAction)
    expect(screen.getByTestId(`express-block`).dataset.status).toBe(`ok`)
    expect(screen.getByTestId(`call-view`).textContent).not.toContain(`<a2ui>`)
    act(() => {
      fireEvent.click(screen.getByRole(`button`, { name: `Ship it` }))
    })
    expect(onSurfaceAction.mock.calls[0]?.[1]).toEqual({ surfaceId: `m:v7#1`, block: `m:v7#1`, messageId: `v7` })
  })

  it(`the summary reads its pure prose; a block-only summary draws no band`, () => {
    const { unmount } = body(SAMPLE)
    expect(screen.getByTestId(`call-view-summary`).textContent).toContain(`Checked the three lanes.\n\nAll green.`)
    unmount()
    body(`<a2ui>\nroot = Text(text="x")\n</a2ui>`)
    expect(screen.queryByTestId(`call-view-summary`)).toBeNull()
  })
})

describe(`SessionCard caption`, () => {
  it(`reads the pure prose of an agent's text (a call's summary)`, () => {
    render(
      <SessionCard
        card={{
          sessionId: `call1`,
          icon: `voice-call`,
          name: `Call · 4 min`,
          kindWord: ``,
          state: `done`,
          tone: `muted`,
          elapsed: null,
          running: false,
          testId: `session-card`,
          caption: `Checked the three lanes.\n<a2ui>\nroot = Text(text="x")\n</a2ui>`,
        } as never}
      />
    )
    const caption = document.querySelector(`[data-slot="session-card-caption"]`)
    expect(caption?.textContent).toBe(`Checked the three lanes.`)
  })
})
