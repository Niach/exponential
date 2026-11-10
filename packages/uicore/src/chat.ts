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
  /** The session the relay's name line opens (#EXP-1772: set = a relay, another session's message). */
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
  /** #EXP-1772: THE inline marker (an event line, a system note): a centred muted label between two hairlines. */
  | { id: string; type: `marker`; label: string }
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
  /** #EDE-65 lane O: the "+" menu's rows the host serves (composer-menu.json, resolved), in order. */
  plus?: ChatPlusRow[]
  /** The quick replies over the composer (the newest message's). */
  replies?: ChatRepliesInput | null
  /** The host paints the context ring beside Send (an Embed it claims). */
  ring?: boolean
}

/** One "+" row: `separator` = a separator; `items` = a submenu's entries (absent = an action row). */
export interface ChatPlusRow {
  id: string
  label?: string
  icon?: string
  items?: ChatPlusItem[] | null
  /** The submenu filters its entries in a search field. */
  searchable?: boolean
}

/** A submenu entry: `checked` = a check entry, absent = an action. */
export interface ChatPlusItem {
  label: string
  value: string
  icon?: string | null
  checked?: boolean | null
  disabled?: boolean
}

export interface ChatRepliesInput {
  messageId: string
  items: { id: string; text: string }[]
}

/** One quick reply as the document's chip draws it (its message rides along). */
export interface ChatReply {
  key: string
  label: string
  messageId: string
}

/** A genUI menuItem. */
export type ChatMenuItem = Record<string, unknown>

/** The "+" rows as genUI menuItems: a separator; an action row = its id as the value; a submenu row = its entries, each `<row id>:<value>` (a check entry a checkbox), `searchable` when it filters. */
export function plusMenu(rows: ChatPlusRow[]): ChatMenuItem[] {
  return rows.map((row) => {
    if (row.id === `separator`) return { kind: `separator` }
    const item: ChatMenuItem = { label: row.label ?? ``, value: row.id }
    if (row.icon) item.icon = row.icon
    if (row.items) {
      item.kind = `submenu`
      if (row.searchable) item.searchable = true
      item.items = row.items.map((entry) => {
        const e: ChatMenuItem = { label: entry.label, value: `${row.id}:${entry.value}` }
        if (entry.icon) e.icon = entry.icon
        if (entry.checked !== undefined && entry.checked !== null) {
          e.kind = `checkbox`
          e.checked = entry.checked
        }
        if (entry.disabled) e.disabled = true
        return e
      })
    }
    return item
  })
}

/** A "+" pick (`select` {value}): the row's id and, for a submenu entry, its own value (the first `:` splits them). */
export function plusPick(value: string): [string, string | null] {
  const at = value.indexOf(`:`)
  return at < 0 ? [value, null] : [value.slice(0, at), value.slice(at + 1)]
}

export type ChatRowKind = `user` | `agent` | `line` | `marker` | `pill` | `card` | `question` | `picture` | `host`

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
  /** #EXP-1772: a relay's name line ABOVE the bubble (the name + `copy.linkChevron`), the press that opens `link`; blank otherwise. */
  relay: string
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
  composer: {
    placeholder: string
    canSend: boolean
    busy: boolean
    /** #EDE-65 lane T: a send in flight (the IDE's `sending`): the "+" is inert until it lands. */
    sending: boolean
    /** #EDE-65 lane O: the "+" Menu's items (the host sets them as the Menu's literal prop), `plus` = any. */
    plus: boolean
    menu: ChatMenuItem[]
    /** The quick replies' chips and their message (`` = none). */
    replies: ChatReply[]
    repliesMessageId: string
    ring: boolean
  }
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
  relay: ``,
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
      const relay = (group.link ?? ``) !== ``
      const name = bubbles && side === `in` ? (input.name ?? ``) : ``
      const grouped = (kind: ChatRowKind): ChatRow => ({
        ...blank(input.id, kind),
        author: relay ? `` : name,
        relay: relay && name.trim() !== `` ? `${name.trim()} ${COPY.linkChevron}` : ``,
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
      return { ...blank(input.id, `marker`), text: input.body }
    }
    case `host`:
      return blank(input.id, `host`)
    case `pill`:
      return { ...blank(input.id, `pill`), icon: input.icon, text: input.label }
    case `line`:
      return { ...blank(input.id, `line`), icon: input.icon, text: input.text }
    case `marker`:
      return { ...blank(input.id, `marker`), text: input.label }
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
      sending: input.sending,
      plus: (input.plus ?? []).length > 0,
      menu: plusMenu(input.plus ?? []),
      replies: (input.replies?.items ?? []).map((item) => ({ key: item.id, label: item.text, messageId: input.replies!.messageId })),
      repliesMessageId: input.replies && input.replies.items.length > 0 ? input.replies.messageId : ``,
      ring: (input.ring ?? false) && !home,
    },
  }
}
