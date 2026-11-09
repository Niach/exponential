// #EDE-65 lane I: what the shared chat document binds, a pure projection of
// one conversation the host already resolved. `fixtures/chat.json` is the
// spec; the document (`docs/chat.json`) binds the result under /chatting.
import fixture from "../fixtures/chat.json" with { type: "json" }

export interface ChatAgentInput {
  id: string
  name: string
  icon: string
}

export interface ChatOptionInput {
  label: string
  description: string | null
}

/** A card's genUI surface (Round 9 §19 Embed `surface`): the document draws it in place. */
export interface ChatSurface {
  root: Record<string, unknown>
  data?: Record<string, unknown> | null
  version?: string
}

/** An option as the document draws it: its row's id rides along (a template cannot read its parent). */
export interface ChatOption extends ChatOptionInput {
  messageId: string
}

/** #EDE-65 lane O: how a message sits in a grouped transcript (transcript-grouping.json, the host resolved it). */
export interface ChatGroup {
  /** `out` = the viewer's own; absent = `out` for `user`, else `in`. */
  side?: string | null
  /** The sender's avatar icon beside a group's first `in` bubble. */
  avatar?: string | null
  /** An `in` row right of the avatar column (the avatar or its spacer). */
  indent?: boolean
  /** The session a press opens. */
  link?: string | null
  /** The space above it in px (0, 3, 8, 12, 16); without a group a row sits 16 below the row before it. */
  gap?: number
}

/** A row without a group's gap: the document's old `lg` list gap. */
const DEFAULT_GAP = 16

/** The space above row `index`: a grouped message's own, a host row none (it paints its own), else 16 below the row before it. */
function gap(input: ChatRowInput, index: number): number {
  if (input.type === `message` && input.group && (input.author === `user` || input.author === `agent`)) return input.group.gap ?? 0
  if (input.type === `host`) return 0
  return index === 0 ? 0 : DEFAULT_GAP
}

export type ChatRowInput =
  | { id: string; type: `message`; author: string; body: string; name: string | null; time: string; queued: boolean; group?: ChatGroup | null }
  /** #EDE-65 lane O: a row the host paints itself (an Embed it claims by its painted id). */
  | { id: string; type: `host` }
  | { id: string; type: `pill`; icon: string; label: string }
  | { id: string; type: `line`; icon: string; text: string }
  | { id: string; type: `card`; icon: string; title: string; text: string; target: string | null; surface?: ChatSurface | null }
  | { id: string; type: `question`; question: string; options: ChatOptionInput[]; custom: boolean; multi: boolean; waiting: boolean; answer: string | null; surface?: ChatSurface | null }
  | { id: string; type: `picture`; attachmentId: string; caption: string | null }

/** What the host knows. */
export interface ChatInput {
  sessionId: string | null
  title: string
  icon: string
  bubbles: boolean
  agents: ChatAgentInput[]
  agentId: string | null
  draft: string
  sending: boolean
  busy: boolean
  /** The host's composer has the `/` menu (the IDE); eDE until it does not. */
  slashCommands: boolean
  rows: ChatRowInput[]
}

export type ChatRowKind = `user` | `agent` | `line` | `pill` | `card` | `question` | `picture` | `host`

export interface ChatRow {
  id: string
  kind: ChatRowKind
  bubble: boolean
  body: string
  caption: string
  queued: boolean
  icon: string
  title: string
  text: string
  target: string | null
  options: ChatOption[]
  custom: boolean
  multi: boolean
  waiting: boolean
  answer: string | null
  attachmentId: string | null
  surface: ChatSurface | null
  /** #EDE-65 lane O: a bubble's MessageBubble props (bubbles only). */
  side: string
  author: string
  time: string
  /** The message's ChatGroup, passed through. */
  avatar: string
  indent: boolean
  link: string | null
  gap: number
}

/** What the document binds. */
export interface ChatView {
  face: `home` | `conversation`
  header: { title: string; icon: string }
  agentRow: { label: string; agents: { id: string; name: string; icon: string; selected: boolean }[] }
  rows: ChatRow[]
  empty: boolean
  composer: { placeholder: string; canSend: boolean; busy: boolean }
}

const COPY = fixture.copy

const blank = (id: string, kind: ChatRowKind): ChatRow => ({
  id,
  kind,
  bubble: false,
  body: ``,
  caption: ``,
  queued: false,
  icon: ``,
  title: ``,
  text: ``,
  target: null,
  options: [],
  custom: false,
  multi: false,
  waiting: false,
  answer: null,
  attachmentId: null,
  surface: null,
  side: ``,
  author: ``,
  time: ``,
  avatar: ``,
  indent: false,
  link: null,
  gap: 0,
})

/** `name · time`, a missing part dropped with its separator. */
const caption = (...parts: (string | null)[]): string =>
  parts.map((part) => (part ?? ``).trim()).filter((part) => part !== ``).join(` · `)

function row(input: ChatRowInput, bubbles: boolean): ChatRow {
  switch (input.type) {
    case `message`: {
      const group = input.group ?? {}
      const side = group.side === `in` || group.side === `out` ? group.side : input.author === `user` ? `out` : `in`
      const grouped = (kind: ChatRowKind): ChatRow => ({
        ...blank(input.id, kind),
        author: bubbles && side === `in` ? (input.name ?? ``) : ``,
        time: bubbles ? input.time.trim() : ``,
        side: bubbles ? side : ``,
        avatar: group.avatar ?? ``,
        indent: group.indent ?? false,
        link: group.link ?? null,
        gap: group.gap ?? 0,
      })
      if (input.author === `user`) {
        return { ...grouped(`user`), bubble: bubbles, body: input.body, caption: bubbles ? caption(input.time) : caption(input.name, input.time), queued: input.queued }
      }
      if (input.author === `agent`) {
        return { ...grouped(`agent`), bubble: bubbles, body: input.body, caption: bubbles ? caption(input.time) : `` }
      }
      return { ...blank(input.id, `line`), icon: COPY.systemIcon, text: input.body }
    }
    case `host`:
      return blank(input.id, `host`)
    case `pill`:
      return { ...blank(input.id, `pill`), icon: input.icon, text: input.label }
    case `line`:
      return { ...blank(input.id, `line`), icon: input.icon, text: input.text }
    case `card`:
      return { ...blank(input.id, `card`), icon: input.icon, title: input.title, text: input.text, target: input.target, surface: input.surface ?? null }
    case `question`:
      return {
        ...blank(input.id, `question`),
        icon: COPY.questionIcon,
        title: input.question,
        options: input.options.map((o) => ({ ...o, messageId: input.id })),
        custom: input.custom,
        multi: input.multi,
        waiting: input.waiting,
        answer: input.answer,
        surface: input.surface ?? null,
      }
    case `picture`:
      return { ...blank(input.id, `picture`), attachmentId: input.attachmentId, text: input.caption ?? `` }
  }
}

export function chatView(input: ChatInput): ChatView {
  const home = input.sessionId === null
  const picked = input.agents.find((a) => a.id === input.agentId) ?? input.agents[0] ?? null
  const rows = home ? [] : input.rows.map((r, i) => ({ ...row(r, input.bubbles), gap: gap(r, i) }))
  return {
    face: home ? `home` : `conversation`,
    header: { title: input.title, icon: input.icon },
    agentRow: {
      label: picked ? COPY.to.replace(`{name}`, picked.name) : ``,
      agents: input.agents.map((a) => ({ ...a, selected: a.id === picked?.id })),
    },
    rows,
    empty: rows.length === 0,
    composer: {
      placeholder: home ? COPY.homePlaceholder : input.slashCommands ? COPY.placeholder : COPY.messagePlaceholder.replace(`{name}`, input.title),
      canSend: input.draft.trim() !== `` && !input.sending && (!home || picked !== null),
      busy: input.busy,
    },
  }
}
