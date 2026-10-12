import { contract } from "@exp/domain-contract"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import type { IconConcept } from "@exp/icons"
import {
  AccountPicker,
  Composer,
  ComposerSubmit,
  ComposerTool,
  DevicePicker,
  Label,
  MenuPanel,
  ModelPicker,
  Pill,
  Switch,
  conceptIcon,
  type MenuEntry,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1019 — the START-CODING DIALOG; EXP-1249 — its ONE "+" menu.
//
// Drawn from the REAL parts: `Composer` / `ComposerTool` / `ComposerSubmit`
// are the card the app's `LaunchComposer` renders, the options line is the
// typed pickers in their inline variant, and the open "+" is `MenuPanel` (the
// app's `Menu` renderer at rest) fed the rows of `composer-menu.json`, the
// fixture the four clients replay. The words are the contract's
// (`contract.composerUi`, ×4) so the demo cannot drift from what the clients
// print. The app component itself lives in apps/web and cannot be imported
// here.

const { composerUi } = contract
const noop = (): void => {}

const UiAddIcon = conceptIcon(`ui-add`)
const ActionIcon = conceptIcon(`pr-merged`)

const DEVICES = [{ id: `mint`, name: `mint — This device`, kind: `desktop` }]
const ACCOUNTS = [{ key: `claude:1`, agent: `claude`, email: `danny@example.dev` }]
const MODELS = [
  { value: `default`, label: `CLI default` },
  { value: `fable`, label: `Fable` },
]

/** The value a row shows in the specimen (the app reads the model). */
const SAMPLE_VALUES: Record<string, string> = {
  effort: `High`,
  subagents: `Opus`,
  "mcp-servers": `2`,
}
const SAMPLE_CHECKED: Record<string, boolean> = {
  ultracode: false,
  "computer-use": true,
}

/** The "+" rows straight from the fixture, every condition met. */
function composerMenuEntries(): MenuEntry[] {
  return composerMenu.rows.map((row): MenuEntry => {
    if (row.kind === `separator`) return { kind: `separator` }
    const base = {
      id: row.id,
      label: row.label ?? ``,
      icon: conceptIcon(row.icon as IconConcept),
    }
    if (row.kind === `toggle`) {
      return {
        ...base,
        kind: `toggle`,
        checked: SAMPLE_CHECKED[row.id ?? ``] ?? false,
        onChange: noop,
      }
    }
    if (row.kind === `submenu`) {
      return { ...base, kind: `submenu`, value: SAMPLE_VALUES[row.id ?? ``], entries: [] }
    }
    return { ...base, kind: `item`, onSelect: noop }
  })
}

function OptionsLine() {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs text-muted-foreground">
      <DevicePicker triggerVariant="inline" devices={DEVICES} value="mint" onChange={noop} width="sm" />
      <AccountPicker variant="inline" value="claude:1" options={ACCOUNTS} onChange={noop} />
      <ModelPicker models={MODELS} value="fable" onChange={noop} />
      <Label className="gap-1.5 font-normal">
        <span>Plan</span>
        <Switch size="sm" checked={false} aria-label="Plan mode" />
      </Label>
    </div>
  )
}

function Card({ placeholder }: { placeholder: string }) {
  return (
    <Composer
      tools={
        <ComposerTool aria-label={composerMenu.plusLabel} title={composerMenu.plusLabel}>
          <UiAddIcon />
        </ComposerTool>
      }
      submit={<ComposerSubmit aria-label="Start" title="Start" />}
    >
      <div className="min-h-20 px-3 pt-3 text-sm text-muted-foreground">{placeholder}</div>
    </Composer>
  )
}

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `composer-dialog`,
  section: `special`,
  owner: `EXP-1019`,
  title: `Composer dialog`,
  blurb: `THE launcher, opened over wherever the play button was pressed instead of navigating away from it. The SUBJECT leads: the contract's verb (Run for an action, Implement for issues, ×4) followed by the removable subject chips, set as the biggest thing in the dialog. The field under it is the SECONDARY half and says so ("Additional instructions (optional)…", or an action's own promptPlaceholder). The card's ONE tool is the "+" (EXP-1249): Implement issue › and Run action › (SLOP-28: both THE search list, see Search list and Menu submenu), Add file or image, then the run's options (Effort ›, Subagents ›, Ultracode) and tools (MCP servers ›, Computer use for this run) — a dropdown at the "+" from md up, the bottom sheet of the same rows on a phone. Under the card the muted options line: device, account, model, Plan (Resume and the repository when they apply), no overflow. The Agent page around the card is the home column (see Agent home): no suggestion rows, no watermark.`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchDialogHost / LaunchComposer / ComposerPlusMenu`,
      file: `apps/web/src/components/launch-dialog/launch-dialog.tsx`,
      note: `the card is launch-composer.tsx, its "+" launch-dialog/composer-plus-menu.tsx (rows: composer-menu.ts over fixtures/composer-menu.json)`,
    },
    desktop: {
      state: `ok`,
      symbol: `composer_dialog::open`,
      file: `apps/desktop/crates/ui/src/composer_dialog.rs`,
      note: `one launcher in two presentations (chat_screen.rs); the "+" = chat_screen::plus_menu over composer-menu.json`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentComposerHeadline / ComposerPlusMenu`,
      file: `apps/ios/Exponential/UI/Agent/AgentComposerCard.swift`,
      note: `a pushed page on the phone; the "+" sheet = ComposerPlusMenu.swift over composer-menu.json`,
    },
    android: {
      state: `ok`,
      symbol: `AgentComposerHeadline / ComposerPlusMenuSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentComposer.kt`,
      note: `a screen on the phone; the "+" sheet = ComposerPlusMenu.kt over composer-menu.json`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-8">
      <div className="grid w-[34rem] max-w-full gap-2">
        <Caption>An action: one chip, and the send is the only thing left to do.</Caption>
        <div className="flex flex-wrap items-center gap-2 px-1">
          <span className="text-lg font-semibold">{composerUi.runHeadline}</span>
          <Pill size="sm" mode="readonly">
            <ActionIcon aria-hidden />
            Fix merge conflicts
          </Pill>
        </div>
        <Card placeholder={composerUi.instructionsPlaceholder} />
        <OptionsLine />
      </div>
      <div className="grid gap-2">
        <MenuPanel entries={composerMenuEntries()} density="pointer" />
        <Caption>the "+" open: composer-menu.json, every condition met</Caption>
      </div>
      <div className="grid w-[34rem] max-w-full gap-2">
        <Caption>A chat in the dialog: no subject, the contract's placeholder.</Caption>
        <Card placeholder={composerUi.chatPlaceholder} />
        <OptionsLine />
      </div>
    </div>
  ),
}
