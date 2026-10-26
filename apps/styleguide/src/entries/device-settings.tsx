import {
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
  ModelPicker,
  SubShellHost,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1020 fills this entry (EXP-1029 pre-registered it). The composition is
// hand-built from the real `@exp/ui` rows rather than the dialog itself: the
// dialog is wired to Electric and tRPC, and what this entry documents is the
// ORDER and the wording the four clients agreed on, not the plumbing.

const noop = () => {}

/** THE model choices (contract `codingModel`, no Default row). */
const MODELS = [
  { value: `fable`, label: `Fable` },
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
  { value: `haiku`, label: `Haiku` },
]
const RemoveIcon = conceptIcon(`ui-delete`)

export const entry: StyleguideEntry = {
  id: `device-settings`,
  section: `special`,
  owner: `EXP-1020`,
  title: `Device settings`,
  blurb: `One layout on all four clients (EXP-1020). Top to bottom: the identity row, the default-device toggle, sharing (server machines), then the computer-use card (the switch and, while it is on, the model the session's screen-driving subagents use, EXP-1236), then the agent-defaults card — agent tabs, model, thread model (the model a thread's or team agent's dispatcher root runs on: Haiku for Claude, the codex models for Codex), subagent model (claude only), effort, ultracode, plan mode — and nothing after it. Every model row is THE model picker over the contract list, no Default row. Update comes last but one; "Remove device" is a plain row, not a section of its own. No worktrees: a machine's worktrees are the IDE's local Settings → Worktrees. No Voice: it is the member's Settings → Voice (NEW-67, voice-settings). A headline only appears where something else shares the page, which is why the agent card has none.`,
  status: {
    web: {
      state: `ok`,
      symbol: `DeviceSettingsDialog`,
      file: `apps/web/src/components/device-settings-dialog.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `DeviceSettingsView`,
      file: `apps/desktop/crates/ui/src/device_settings.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `DeviceSettingsSheet`,
      file: `apps/ios/Exponential/UI/Session/DeviceSettingsSheet.swift`,
    },
    android: {
      state: `ok`,
      symbol: `DeviceSettingsSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/DeviceSettingsSheet.kt`,
    },
  },
  island: () => (
    <SubShellHost className="w-[24rem]">
      <GlassGroup>
        <GlassToggleRow
          id="demo-device-settings-default"
          label="Default device"
          checked
          onCheckedChange={noop}
        />
      </GlassGroup>
      <GlassSectionHeader label="Sharing" />
      <GlassGroup>
        <GlassToggleRow
          id="demo-device-settings-share"
          label="Acme"
          checked
          onCheckedChange={noop}
        />
      </GlassGroup>
      {/* EXP-1236: the model row shows only while the switch is on; the
          aliases are contract `codingModel`, Haiku the default. */}
      <GlassGroup>
        <GlassToggleRow
          id="demo-device-settings-computer-use"
          label="Computer use"
          checked
          onCheckedChange={noop}
        />
        <ModelPicker
          triggerVariant="row"
          width="md"
          mobileTitle="Computer use model"
          value="haiku"
          onChange={noop}
          models={MODELS}
        />
      </GlassGroup>
      <GlassGroup>
        <ModelPicker
          triggerVariant="row"
          width="md"
          value="fable"
          onChange={noop}
          models={MODELS}
        />
        <ModelPicker
          triggerVariant="row"
          width="md"
          mobileTitle="Thread model"
          value="haiku"
          onChange={noop}
          models={MODELS}
        />
        <ModelPicker
          triggerVariant="row"
          width="md"
          mobileTitle="Subagent model"
          value="opus"
          onChange={noop}
          models={MODELS}
        />
        <GlassToggleRow
          id="demo-device-settings-ultracode"
          label="Ultracode"
          checked={false}
          onCheckedChange={noop}
        />
      </GlassGroup>
      <GlassGroup>
        <div className="flex items-center gap-3 px-4 py-3 text-sm text-destructive">
          <RemoveIcon className="size-3.5" />
          Remove device
        </div>
      </GlassGroup>
    </SubShellHost>
  ),
}
