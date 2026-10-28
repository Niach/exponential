import {
  WORK_SCREEN_COPY,
  workScreenOptions,
} from "@exp/domain-contract/work-screen"
import { GlassGroup, GlassToggleRow, ModelPicker, Picker, SubShellHost } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-59 — THE work screen row: the display coding agents drive with computer
// use, picked in Device settings' computer-use card under the model row.
// Options are the reference's `workScreenOptions` (fixtures/work-screen.json),
// so the specimen can never drift from the rule; the picker is the dialog's
// own (`Picker` row, inline under its trigger as it opens in a dialog).

const noop = () => {}

const MODELS = [
  { value: `fable`, label: `Fable` },
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
  { value: `haiku`, label: `Haiku` },
]

/** The fixture's two-display case: display 2 is the primary. */
const DISPLAYS = [
  { id: `uuid-a`, number: 1, width: 2560, height: 1440, primary: false },
  { id: `uuid-b`, number: 2, width: 3024, height: 1964, primary: true },
]

const ITEMS = workScreenOptions(DISPLAYS, null).map((option) => ({
  value: option.value ?? `auto`,
  label: option.label,
}))

export const entry: StyleguideEntry = {
  id: `work-screen`,
  section: `special`,
  owner: `NEW-59`,
  title: `Work screen`,
  blurb: `The display agents drive with computer use (NEW-59, work-screen.json). Device settings → the computer-use card carries a "${WORK_SCREEN_COPY.row}" picker row directly under the Computer use model row, only while computer use is on, on all four clients. Its options: "${WORK_SCREEN_COPY.auto}" first (every display without a fullscreen app), then one row per display the device reports in its doctor, "${WORK_SCREEN_COPY.display}" · width × height, the primary one ending in " · ${WORK_SCREEN_COPY.primary}"; a stored display the device no longer reports stays pickable last as "${WORK_SCREEN_COPY.notConnected}". The row's value is the picked option's label; the pick rides the dialog's launch-defaults save as the display's stable id (launch_defaults.computerUseDisplayId, null = Auto; #FEED-161: plugging in or reordering displays never moves it to another monitor). The desktop computer hub enforces it: agents only see and drive the work screen, and never click or type while the person is in a fullscreen app on another display.`,
  status: {
    web: {
      state: `ok`,
      symbol: `DeviceSettingsDialog`,
      file: `apps/web/src/components/device-settings-dialog.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `render`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/work_screen.rs`,
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
          id="demo-work-screen-computer-use"
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
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          presentation="inline"
          open
          onOpenChange={noop}
          mobileTitle={WORK_SCREEN_COPY.row}
          value="auto"
          items={ITEMS}
          onChange={noop}
        />
      </GlassGroup>
    </SubShellHost>
  ),
}
