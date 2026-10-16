import { contract } from "@exp/domain-contract"
import {
  AccountPicker,
  AgentRunMark,
  Button,
  Composer,
  ComposerSubmit,
  ComposerTool,
  ContextRing,
  DevicePicker,
  IssueChip,
  Label,
  MenuPanel,
  ModelPicker,
  RepositoryPicker,
  Switch,
  conceptIcon,
  stripTabClass,
  cn,
  type MenuEntry,
} from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// EXP-1334 DRAFT (removed before merge): the thread header's strip without a
// scroll, three ways to open a session's nested tree on a CLICK, the "+"
// left of Conversation, and ONE composer (the old session layout: the card,
// options under it) for a start, a running session and an idle thread.

const noop = (): void => {}
const ConversationIcon = conceptIcon(`action-chat`)
const GuideIcon = conceptIcon(`guide-changes`)
const AddIcon = conceptIcon(`ui-add`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const SearchIcon = conceptIcon(`nav-search`)
const AttachIcon = conceptIcon(`ui-attach`)
const ImageIcon = conceptIcon(`editor-image`)
const HashIcon = conceptIcon(`editor-issue-ref`)
const EmojiIcon = conceptIcon(`editor-emoji`)

const IN_PROGRESS = { icon: `progress-2-4`, colorClass: `text-amber-400` } as const
const BACKLOG = { icon: `circle-dashed`, colorClass: `text-muted-foreground` } as const

const ISSUES = [
  { identifier: `EXP-1321`, title: `no task ui`, status: IN_PROGRESS },
  { identifier: `EXP-1323`, title: `Threads implementation`, status: IN_PROGRESS },
  { identifier: `EXP-1324`, title: `Trace and CodeBlock`, status: BACKLOG },
  { identifier: `EXP-1328`, title: `Suggested tasks`, status: BACKLOG },
]

type Variant = `split` | `dropdown` | `badge`

function Divider() {
  return <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />
}

function Tab({ active, children }: { active?: boolean; children: ReactNode }) {
  return (
    <Button type="button" variant="ghost" className={stripTabClass(Boolean(active))}>
      {children}
    </Button>
  )
}

function More({ count, label }: { count: number; label: string }) {
  return (
    <Button
      type="button"
      variant="ghost"
      aria-label={label}
      className={cn(stripTabClass(false, true), `px-2 text-muted-foreground`)}
    >
      +{count}
    </Button>
  )
}

/** One session chip in the three click affordances. */
function SessionChip({
  variant,
  name,
  path,
  nested,
  active,
  state,
}: {
  variant: Variant
  name: string
  /** The focused descendant (`Parent › Child`). */
  path?: string
  nested: number
  active?: boolean
  state?: `working` | `needs_input`
}) {
  const label = (
    <>
      <AgentRunMark agent="claude" state={state} ringClassName="ring-background" className="size-3.5" />
      <span className="min-w-0 truncate">
        {name}
        {path && <span className="text-muted-foreground"> › {path}</span>}
      </span>
    </>
  )
  if (variant === `split`) {
    return (
      <span
        className={cn(
          stripTabClass(Boolean(active), true),
          `max-w-[15rem] gap-0 p-0`
        )}
      >
        <span className="flex min-w-0 items-center gap-1.5 px-2.5">{label}</span>
        {nested > 0 && (
          <span className="flex h-full items-center border-l border-glass-stroke px-1.5 text-muted-foreground">
            <ChevronDownIcon className="size-3.5" />
          </span>
        )}
      </span>
    )
  }
  return (
    <span className={cn(stripTabClass(Boolean(active), true), `max-w-[15rem]`)}>
      {label}
      {nested > 0 &&
        (variant === `dropdown` ? (
          <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <span className="shrink-0 rounded-sm bg-muted px-1 font-mono text-[0.6875rem] text-muted-foreground">
            {nested}
          </span>
        ))}
    </span>
  )
}

/** The md+ strip at one width tier: `full` titles, `ident` only, `fold` = +X. */
function Strip({
  variant,
  tier,
  width,
}: {
  variant: Variant
  tier: `full` | `ident` | `fold`
  width: string
}) {
  const shown = ISSUES.slice(0, tier === `full` ? 2 : tier === `ident` ? 2 : 1)
  return (
    <div
      className="flex items-center gap-1 rounded-lg border border-dashed border-border/60 p-1.5"
      style={{ width }}
    >
      <Button variant="ghost" size="icon-xs" aria-label="Add" className="shrink-0 text-muted-foreground">
        <AddIcon />
      </Button>
      <Tab>
        <ConversationIcon className="size-3.5 shrink-0" />
        Conversation
      </Tab>
      <Divider />
      <div className="flex min-w-0 shrink items-center gap-1 overflow-hidden">
        {shown.map((issue) => (
          <IssueChip
            key={issue.identifier}
            identifier={issue.identifier}
            title={issue.title}
            status={issue.status}
            size={tier === `full` ? `tab` : `pager`}
          />
        ))}
      </div>
      <More count={ISSUES.length - shown.length} label="more issues" />
      <Divider />
      <div className="flex shrink-0 items-center gap-1">
        <SessionChip variant={variant} name="Native shots" nested={2} state="working" />
        <SessionChip
          variant={variant}
          name="Threads impl"
          path={tier === `full` ? `Android lane` : undefined}
          nested={3}
          active
          state="needs_input"
        />
        {tier !== `full` && <More count={2} label="2 more sessions" />}
      </div>
      <Divider />
      <Tab>
        <GuideIcon className="size-3.5 shrink-0" />
        Guide
        <span className="size-1.5 shrink-0 rounded-full bg-emerald-400" />
      </Tab>
    </div>
  )
}

/** The phone pager: the same chips, Conversation + Guide pinned. */
function PhoneStrip({ variant }: { variant: Variant }) {
  return (
    <div className="flex w-[33rem] items-center gap-1 rounded-lg border border-dashed border-border/60 p-1.5">
      <Button variant="ghost" size="icon-xs" aria-label="Add" className="shrink-0 text-muted-foreground">
        <AddIcon />
      </Button>
      <Tab>Conversation</Tab>
      <div className="flex min-w-0 shrink items-center gap-1 overflow-hidden">
        <IssueChip identifier="EXP-1321" title="no task ui" status={IN_PROGRESS} size="pager" />
      </div>
      <More count={3} label="3 more issues" />
      <div className="flex shrink-0 items-center gap-1">
        <SessionChip variant={variant} name="Threads" nested={3} active state="needs_input" />
        <More count={1} label="1 more session" />
      </div>
      <Tab>Guide</Tab>
    </div>
  )
}

function treeEntries(): MenuEntry[] {
  const row = (id: string, label: string, depth: number, checked = false): MenuEntry => ({
    kind: `item`,
    id,
    label: <span style={{ paddingLeft: `${depth * 1}rem` }}>{label}</span>,
    icon: <AgentRunMark agent="claude" ringClassName="ring-popover" className="size-3.5" />,
    checked,
    onSelect: noop,
  })
  return [
    row(`a`, `Threads impl`, 0),
    row(`b`, `Web lane`, 1),
    row(`c`, `Android lane`, 1, true),
    row(`d`, `Emulator shots`, 2),
  ]
}

function plusEntries(): MenuEntry[] {
  return [
    { kind: `submenu`, id: `link`, label: `Link an issue`, icon: SearchIcon, entries: [] },
    { kind: `item`, id: `new`, label: `New issue`, icon: AddIcon, onSelect: noop },
  ]
}

const MODELS = contract.codingModel.values.map((value) => ({
  value,
  label: value.charAt(0).toUpperCase() + value.slice(1),
}))

/** ONE composer: the card (field, inline tools, send), options UNDER it. */
function OneComposer({
  placeholder,
  phase,
}: {
  placeholder: string
  phase: `start` | `running` | `idle`
}) {
  return (
    <div className="grid w-[36rem] max-w-full gap-1.5">
      <Composer
        tools={
          <>
            <ComposerTool aria-label="Options" title="Options">
              <AddIcon />
            </ComposerTool>
            <ComposerTool aria-label="Add image" title="Add image">
              <ImageIcon />
            </ComposerTool>
            <ComposerTool aria-label="Attach file" title="Attach file">
              <AttachIcon />
            </ComposerTool>
            <ComposerTool aria-label="Link an issue" title="Link an issue">
              <HashIcon />
            </ComposerTool>
            <ComposerTool aria-label="Emoji" title="Emoji">
              <EmojiIcon />
            </ComposerTool>
          </>
        }
        submit={<ComposerSubmit stop={phase === `running`} aria-label={phase === `running` ? `Stop` : `Send`} />}
      >
        <div className="min-h-16 px-3 pt-3 text-sm text-muted-foreground">{placeholder}</div>
      </Composer>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-2 text-xs text-muted-foreground">
        {phase === `start` && (
          <>
            <DevicePicker
              triggerVariant="inline"
              devices={[{ id: `mint`, name: `macbook`, kind: `desktop` }]}
              value="mint"
              onChange={noop}
              width="sm"
            />
            <AccountPicker
              variant="inline"
              value="claude:1"
              options={[{ key: `claude:1`, agent: `claude`, email: `danny@example.dev` }]}
              onChange={noop}
            />
          </>
        )}
        <ModelPicker models={MODELS} value={phase === `idle` ? `haiku` : `opus`} onChange={noop} />
        <Label className="gap-1.5 font-normal">
          <span>Plan</span>
          <Switch size="sm" checked={phase === `running`} aria-label="Plan mode" />
        </Label>
        {phase === `start` && (
          <RepositoryPicker
            triggerVariant="inline"
            width="sm"
            repositories={[{ id: `exp`, fullName: `Niach/exponential` }]}
            value="exp"
            onChange={noop}
          />
        )}
        {phase !== `start` && (
          <span className="ml-auto">
            <ContextRing percent={phase === `running` ? 42 : 12} tone="normal" />
          </span>
        )}
      </div>
    </div>
  )
}

function optionsEntries(): MenuEntry[] {
  return [
    { kind: `submenu`, id: `implement`, label: `Implement issue`, icon: HashIcon, entries: [] },
    { kind: `submenu`, id: `action`, label: `Run action`, icon: conceptIcon(`action-run`), entries: [] },
    { kind: `item`, id: `file`, label: `Add file or image`, icon: AttachIcon, onSelect: noop },
    { kind: `separator` },
    { kind: `submenu`, id: `effort`, label: `Effort`, value: `High`, icon: conceptIcon(`ui-usage`), entries: [] },
    { kind: `toggle`, id: `ultra`, label: `Ultracode`, icon: conceptIcon(`action-default`), checked: false, onChange: noop },
    { kind: `separator` },
    { kind: `submenu`, id: `mcp`, label: `MCP servers`, value: `2`, icon: conceptIcon(`ui-mcp`), entries: [] },
    { kind: `toggle`, id: `cu`, label: `Computer use`, icon: conceptIcon(`nav-computer`), checked: true, onChange: noop },
  ]
}

function Caption({ children }: { children: ReactNode }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

function VariantBlock({ variant, title }: { variant: Variant; title: string }) {
  return (
    <div className="grid gap-2" data-testid={`draft-variant-${variant}`}>
      <p className="text-sm font-semibold">{title}</p>
      <Strip variant={variant} tier="full" width="68rem" />
      <Strip variant={variant} tier="ident" width="58rem" />
      <Strip variant={variant} tier="fold" width="50rem" />
      <PhoneStrip variant={variant} />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `thread-header-drafts`,
  section: `special`,
  owner: `EXP-1334`,
  title: `DRAFT: thread header + one composer`,
  blurb: `EXP-1334 draft. The strip never scrolls: "+" and Conversation pinned left, Guide pinned right; issues then sessions between, each zone shrinking full → identifier → "+N". Session chips = the dispatcher's direct children; a chip with descendants opens THE Menu with its subtree on a click (a bottom sheet on phones); a focused grandchild keeps its top-level chip active as "Parent › Child". Three click affordances: A split chip, B whole-chip dropdown, C count badge.`,
  status: {
    web: { state: `ok`, symbol: `draft`, file: `apps/styleguide/src/entries/thread-header-drafts.tsx` },
    desktop: { state: `n/a`, note: `draft` },
    ios: { state: `n/a`, note: `draft` },
    android: { state: `n/a`, note: `draft` },
  },
  island: () => (
    <div className="grid gap-10">
      <VariantBlock variant="split" title="A · Split chip: the body opens the session, the ⌄ segment its subtree" />
      <VariantBlock variant="dropdown" title="B · Whole-chip dropdown: the chip opens the subtree (row 1 = the session itself)" />
      <VariantBlock variant="badge" title="C · Count badge: the nested count opens the subtree, no chevron" />
      <div className="flex flex-wrap items-start gap-8">
        <div className="grid gap-2">
          <MenuPanel entries={treeEntries()} density="pointer" />
          <Caption>the subtree open (md+ dropdown)</Caption>
        </div>
        <div className="grid w-[24rem] gap-2">
          <MenuPanel entries={treeEntries()} look="sheet" title="Threads impl" density="touch" />
          <Caption>the same rows on a phone (bottom sheet)</Caption>
        </div>
        <div className="grid gap-2">
          <MenuPanel entries={plusEntries()} density="pointer" />
          <Caption>the "+" (left of Conversation), the IDE's menu ×4</Caption>
        </div>
      </div>
      <div className="flex flex-wrap items-start gap-8">
        <div className="grid gap-2">
          <Caption>Start (Home / New session): options under the card</Caption>
          <OneComposer placeholder="Ask anything, # an issue, / for commands" phase="start" />
          <Caption>Running session: model · Plan switch live mid-run · usage ring</Caption>
          <OneComposer placeholder="Type / for commands" phase="running" />
          <Caption>Idle thread (the dispatcher)</Caption>
          <OneComposer placeholder="Reply…" phase="idle" />
        </div>
        <div className="grid gap-2">
          <MenuPanel entries={optionsEntries()} density="pointer" />
          <Caption>the "+" options: the same rows before and DURING a session</Caption>
        </div>
      </div>
    </div>
  ),
}
