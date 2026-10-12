import type { ReactNode } from "react"
import fixture from "@exp/domain-contract/fixtures/thread-view.json"
import { SEMANTIC_ICONS } from "@exp/icons"
import {
  AgentRunMark,
  ExponentialLogo,
  IssueChip,
  MenuPanel,
  StatusGlyph,
  ThreadFoldedRun,
  ThreadGlyph,
  ThreadHeader,
  ThreadLinkRow,
  ThreadListRow,
  ThreadParallelCard,
  ThreadRunHeader,
  ThreadStrip,
  conceptIcon,
  type MenuEntry,
  type ThreadStripEntry,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1284: the THREAD's Exponential-specific parts, the REAL `@exp/ui`
// components (`thread-parts.tsx`) painted from the contract's own expected
// views (`thread-view.json`, ×4): the child strip of draft 1, the
// conversation of draft 2 (folded sessions, the agent's link rows, the
// parallel card), the thread's ONE menu and the list rows. The web app binds
// them to the synced rows (`apps/web/src/routes/t/$teamSlug/threads/`).

type Expected = (typeof fixture.views)[number][`expected`]
type Input = (typeof fixture.views)[number][`input`]

/** The model's blocks (`lib/thread-view.ts`), typed for the specimen. */
interface LinkModel {
  itemId: string
  issueId: string
  label: string
}
type BlockModel =
  | {
      kind: `run`
      runId: string
      label: string
      caption: string[]
      state: `live` | `needs_input` | `ended`
      folded: boolean
      links: LinkModel[]
    }
  | {
      kind: `parallel`
      label: string
      caption: string[]
      state: `live` | `needs_input` | `ended`
      runs: {
        runId: string
        name: string
        issueId: string | null
        deviceLabel: string | null
        state: string
      }[]
      links: LinkModel[]
    }

const DRAFT_1 = fixture.views[0]!
const DRAFT_2 = fixture.views[1]!
const noop = (): void => {}

const STATUS_ICONS = [`status-in-progress`, `status-backlog`, `status-backlog`] as const

function statusOf(index: number) {
  const icon = SEMANTIC_ICONS[STATUS_ICONS[index % STATUS_ICONS.length]!]
  return { icon, colorClass: index === 0 ? `text-amber-400` : `text-muted-foreground` }
}

function strip(view: Expected, activeIssue: string | null): ThreadStripEntry[] {
  return view.strip.flatMap((child, index): ThreadStripEntry[] => {
    if (child.kind === `conversation`) {
      return [{ kind: `conversation`, active: activeIssue === null, onSelect: noop }]
    }
    if (child.kind === `issue` && `identifier` in child) {
      return [
        {
          kind: `issue`,
          key: child.issueId!,
          identifier: child.identifier!,
          title: child.title!,
          lead: <StatusGlyph {...statusOf(index - 1)} className="size-3.5" />,
          active: activeIssue === child.issueId,
          // The issue the agent just filed fades in with its ring.
          fresh: child.identifier === `EXP-1277`,
          onSelect: noop,
        },
      ]
    }
    if (child.kind === `guide`) {
      return [{ kind: `guide`, active: false, dot: Boolean(child.dot), onSelect: noop }]
    }
    return []
  })
}

function chip(input: Input, issueId: string, short = false): ReactNode {
  const index = input.issues.findIndex((issue) => issue.id === issueId)
  const issue = input.issues[index]
  if (!issue) return null
  return (
    <IssueChip
      identifier={issue.identifier}
      title={issue.title}
      status={statusOf(index)}
      size={short ? `sm` : `md`}
    />
  )
}

const LinkMark = <ExponentialLogo variant="light" size={14} className="size-3.5" />

function Conversation({ input, view }: { input: Input; view: Expected }) {
  const nameOf = (runId: string) => input.runs.find((run) => run.id === runId)?.name ?? ``
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      {(view.conversation as BlockModel[]).map((block, index) => {
        const links = block.links.map((link) => (
          <ThreadLinkRow key={link.itemId} lead={LinkMark} label={link.label} chip={chip(input, link.issueId)} />
        ))
        if (block.kind === `parallel`) {
          return (
            <div key={index} className="space-y-2">
              <ThreadRunHeader
                lead={<AgentRunMark agent="claude" ringClassName="ring-background" />}
                label={block.label}
                caption={block.caption}
                state={block.state}
              />
              <ThreadParallelCard
                rows={block.runs.map((row, rowIndex) => ({
                  key: row.runId,
                  lead: (
                    <AgentRunMark
                      agent="claude"
                      state={row.state === `needs_input` ? `needs_input` : rowIndex === 0 ? `working` : undefined}
                      ringClassName="ring-card"
                    />
                  ),
                  name: row.name,
                  chip: row.issueId ? chip(input, row.issueId, true) : undefined,
                  meta: `${row.deviceLabel} · ${rowIndex === 0 ? `22m` : `9m`}`,
                  onOpen: noop,
                }))}
              />
              {links}
            </div>
          )
        }
        const run = block
        return (
          <div key={index} className="space-y-2">
            <ThreadRunHeader
              lead={
                <AgentRunMark
                  agent="claude"
                  state={run.state === `ended` ? `ended` : `working`}
                  ringClassName="ring-background"
                />
              }
              label={run.label}
              caption={run.caption}
              state={run.state}
              onOpen={noop}
            />
            {run.folded ? (
              <ThreadFoldedRun name={nameOf(run.runId)} onShowWork={noop} />
            ) : (
              <p className="pl-[1.375rem] text-sm text-foreground/90">
                Two separate bugs. I'll file the model picker one and keep both in this thread.
              </p>
            )}
            {links}
          </div>
        )
      })}
    </div>
  )
}

const MENU: MenuEntry[] = [
  { kind: `item`, label: `Link an issue`, icon: conceptIcon(`nav-search`), onSelect: noop },
  { kind: `item`, label: `New issue in this thread`, icon: conceptIcon(`ui-add`), onSelect: noop },
  { kind: `item`, label: `Start a session here`, icon: conceptIcon(`action-run`), onSelect: noop },
  { kind: `separator` },
  { kind: `item`, label: `Unlink EXP-1270`, icon: conceptIcon(`ui-close`), onSelect: noop },
]

export const entry: StyleguideEntry = {
  id: `thread-parts`,
  section: `special`,
  owner: `EXP-1284`,
  title: `Thread`,
  blurb: `The unit of work holding n issues and m sessions. Header: the title + the muted \`thread\` word; under it the CHILD STRIP: Conversation · one tab per issue (status glyph, mono identifier, title) · "+" · Guide (green dot while the thread has a Guide), the header caption trailing; a child the viewer has not seen yet fades in with a ring. The conversation: one block per session (agent glyph, \`Session N\`, device · resumed · started by Session K, a hairline, the state word), ended sessions fold to their name + Show work, the issues the agent linked sit under their block (\`Linked\` / \`Filed and linked\` + the chip), and the sessions one parent started share ONE card. The "+" and every issue tab open the thread's ONE menu. Lists show threads: the title over the list caption. Model = \`buildThreadView()\` (fixture \`thread-view.json\` ×4). Phones: the strip is the pager.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadStrip`,
      file: `packages/ui/src/thread-parts.tsx`,
      note: `page: apps/web/src/routes/t/$teamSlug/threads/$threadId.tsx; model: apps/web/src/lib/thread-view.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `ThreadScreenView`,
      file: `apps/desktop/crates/ui/src/thread_screen.rs`,
      note: `model: crates/domain/src/thread_view.rs; styleguide/entries/thread_view.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/ios/Exponential/UI/Thread/ThreadScreen.swift`,
      note: `model: ExpCore ThreadView.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadScreen.kt`,
      note: `model: domain/ThreadView.kt`,
    },
  },
  island: () => {
    const draft1 = DRAFT_1.expected
    const draft2 = DRAFT_2.expected
    return (
      <div className="flex flex-col gap-8 text-sm">
        <div className="flex max-w-3xl flex-col gap-3">
          <ThreadHeader title={DRAFT_1.input.runs[0]!.name} />
          <ThreadStrip entries={strip(draft1, null)} caption={draft1.headerCaption} />
        </div>
        <div className="flex max-w-3xl flex-col gap-3">
          <ThreadHeader title={draft2.title} />
          <ThreadStrip entries={strip(draft2, null)} caption={draft2.headerCaption} />
          <div className="flex items-start gap-6">
            <Conversation input={DRAFT_2.input} view={draft2} />
            <MenuPanel entries={MENU} className="w-64 shrink-0" />
          </div>
        </div>
        <div className="flex max-w-sm flex-col">
          <ThreadStrip entries={strip(draft1, null)} variant="pager" />
          <ThreadListRow
            lead={<AgentRunMark agent="claude" state="working" ringClassName="ring-background" />}
            title={draft1.title}
            caption={draft1.listCaption}
            onClick={noop}
          />
          <ThreadListRow
            lead={<ThreadGlyph />}
            title={draft2.title}
            caption={draft2.listCaption}
            captionTone="amber"
            onClick={noop}
          />
        </div>
      </div>
    )
  },
}
