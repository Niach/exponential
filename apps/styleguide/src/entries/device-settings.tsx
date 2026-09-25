import {
  AccountPicker,
  Combobox,
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
  SubShell,
  SubShellHost,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1020 fills this entry (EXP-1029 pre-registered it). The composition is
// hand-built from the real `@exp/ui` rows rather than the dialog itself: the
// dialog is wired to Electric and tRPC, and what this entry documents is the
// ORDER and the wording the four clients agreed on, not the plumbing.

const noop = () => {}
const WorkflowIcon = conceptIcon(`nav-workflows`)
const RemoveIcon = conceptIcon(`ui-delete`)

export const entry: StyleguideEntry = {
  id: `device-settings`,
  section: `special`,
  owner: `EXP-1020`,
  title: `Device settings`,
  blurb: `One layout on all four clients (EXP-1020). Top to bottom: the identity row, the default-device toggle, sharing (server machines), then the agent-defaults card — default account (always changeable), agent tabs, model, subagent model (claude only), effort, ultracode, plan mode — ending in the "Workflow settings" SUB-SHELL row. Update comes last but one; "Remove device" is a plain row, not a section of its own. No worktrees: a machine's worktrees are the IDE's local Settings → Worktrees. A headline only appears where something else shares the page, which is why the agent card has none.`,
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
      <GlassGroup>
        <AccountPicker
          variant="row"
          mobileTitle="Default account"
          value="claude:demo"
          options={[{ key: `claude:demo`, agent: `claude`, email: `dev@acme.test` }]}
          onChange={noop}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Model"
          value="opus"
          onChange={noop}
          options={[{ value: `opus`, label: `Opus` }]}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Subagent model"
          value="opus"
          onChange={noop}
          options={[{ value: `opus`, label: `Opus` }]}
        />
        <GlassToggleRow
          id="demo-device-settings-ultracode"
          label="Ultracode"
          checked={false}
          onCheckedChange={noop}
        />
        <SubShell label="Workflow settings" icon={WorkflowIcon} value="opus · fable">
          <GlassGroup>
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Model"
              value="opus"
              onChange={noop}
              options={[{ value: `opus`, label: `Opus` }]}
            />
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Strong model"
              value="fable"
              onChange={noop}
              options={[{ value: `fable`, label: `Fable` }]}
            />
          </GlassGroup>
        </SubShell>
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
