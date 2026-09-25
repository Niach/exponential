import { DevicePicker, PickerList, PickerTrigger, devicePickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* The glyph is the device's own (contract `deviceIcon`), falling back to the
   KIND default — a server reads as a server without its owner picking one. */
const DEVICES = [
  { id: `studio`, name: `Studio`, icon: `laptop` },
  { id: `builder`, name: `builder-01`, kind: `server` },
  {
    id: `old`,
    name: `Old mini`,
    description: `Update to run workflows`,
    disabled: true,
  },
]

export const entry: StyleguideEntry = {
  id: `picker-device`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Device picker`,
  blurb: `The machines a run may start on, each by its device glyph and name — the Agent composer, the automation editor's "Runs on" row and the workflow runner. A machine that cannot take the run is rendered DISABLED with the reason as its muted second line, never dropped: a list that silently shrinks reads as a bug on the machine the user was looking for. A device with no icon of its own falls back to its kind (a server glyph for a server).`,
  status: typedPickerStatus(`DevicePicker`, {
    web: `device-picker.tsx`,
    desktop: `device_picker.rs`,
    ios: `DevicePicker.swift`,
    android: `DevicePicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <DevicePicker
          devices={DEVICES}
          value="studio"
          onChange={noop}
          trigger={<PickerTrigger variant="row" label="Runs on" value="Studio" />}
        />
      }
      surface={
        <PickerList
          mode="single"
          items={devicePickerItems(DEVICES)}
          value="studio"
          onChange={noop}
        />
      }
    />
  ),
}
