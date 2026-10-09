import {
  AccountPicker,
  Button,
  DevicePicker,
  EffortPicker,
  GlassGroup,
  GlassToggleRow,
  Label,
  ModelPicker,
  RepositoryPicker,
  Switch,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// UI cleanup batch — the composer's options line, drawn by the REAL words it
// is made of (apps/web/src/components/launch-dialog/launch-options-line.tsx):
// every word is a typed picker in its `inline` variant — Device, Account,
// Model, Repository — then the Plan switch and the `⋯` for the rest, whose
// body is the same typed pickers as rows (Effort, Subagent model) plus the
// Ultracode switch. Wave B moves that overflow into the "+" menu (EXP-1249).

const noop = (): void => {}
const MoreIcon = conceptIcon(`ui-more`)

const DEVICES = [
  { id: `mint`, name: `mint — This device`, kind: `desktop` },
  { id: `buildbox`, name: `buildbox`, kind: `server` },
]
const ACCOUNTS = [
  { key: `claude:1`, agent: `claude`, email: `danny@example.dev` },
  { key: `codex:1`, agent: `codex`, email: `danny@example.dev` },
]
const MODELS = [
  { value: `default`, label: `CLI default` },
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
]
const REPOS = [
  { id: `exp`, fullName: `Niach/exponential` },
  { id: `site`, fullName: `Niach/website` },
]
const EFFORTS = [
  { value: `default`, label: `CLI default` },
  { value: `high`, label: `High` },
]

export const entry: StyleguideEntry = {
  id: `composer-options`,
  section: `special`,
  owner: `EXP-1249`,
  title: `Composer options`,
  blurb: `The muted line under the composer card, as the real components it is made of: every word is a TYPED picker in its inline variant (DevicePicker, AccountPicker, ModelPicker, RepositoryPicker — the repository only while a chat has several to choose from), each collapsing to plain text once there is one thing it could say; then the Plan switch and the ⋯ whose body is the same pickers as glass rows (EffortPicker, ModelPicker for the subagents) and the Ultracode switch. A pick opens the picker's popover (a bottom sheet on a phone) with the one selection language.`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchOptionsLine`,
      file: `apps/web/src/components/launch-dialog/launch-options-line.tsx`,
      note: `the words are @exp/ui typed pickers (packages/ui/src/picker/model-picker.tsx, repository-picker.tsx, device-picker.tsx)`,
    },
    desktop: {
      state: `ok`,
      symbol: `ChatScreenView::render_options_row`,
      file: `apps/desktop/crates/ui/src/chat_screen.rs`,
      note: `the ⋯ is launch_options::more_options_popover`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentOptionsRow`,
      file: `apps/ios/Exponential/UI/Agent/AgentOptionsRow.swift`,
      note: `pills in a horizontal scroll; the ⋯ is AgentOptionsSheet`,
    },
    android: {
      state: `ok`,
      symbol: `AgentOptionsRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentOptionsRow.kt`,
      note: `same shape; the ⋯ is AgentOptionsSheet`,
    },
  },
  island: () => (
    <div className="grid max-w-[40rem] gap-6">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs text-muted-foreground">
        <DevicePicker
          triggerVariant="inline"
          devices={DEVICES}
          value="mint"
          onChange={noop}
          width="sm"
        />
        <AccountPicker variant="inline" value="claude:1" options={ACCOUNTS} onChange={noop} />
        <ModelPicker models={MODELS} value="opus" onChange={noop} />
        <Label className="gap-1.5 font-normal">
          <span>Plan</span>
          <Switch size="sm" checked={false} aria-label="Plan mode" />
        </Label>
        <RepositoryPicker
          triggerVariant="inline"
          width="sm"
          repositories={REPOS}
          value="exp"
          onChange={noop}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="-my-0.5 text-muted-foreground"
          aria-label="More options"
        >
          <MoreIcon className="size-3.5" />
        </Button>
      </div>
      <div className="w-[20rem] overflow-hidden rounded-lg border border-glass-stroke-card bg-glass-card-opaque">
        <GlassGroup bare>
          <EffortPicker efforts={EFFORTS} value="high" onChange={noop} />
          <ModelPicker
            triggerVariant="row"
            mobileTitle="Subagent model"
            models={MODELS}
            value="default"
            onChange={noop}
          />
          <GlassToggleRow
            id="styleguide-composer-ultracode"
            label="Ultracode"
            checked={false}
            onCheckedChange={noop}
          />
        </GlassGroup>
      </div>
      <p className="text-xs text-muted-foreground">
        The line (one machine would read as plain text) and the ⋯ body.
      </p>
    </div>
  ),
}
