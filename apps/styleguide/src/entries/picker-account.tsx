import { AccountLimitBars, AccountPicker, PickerTrigger } from "@exp/ui"

import { typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* Fractions 0..1, the way a device reports them: the tone ladder is EXP-909's
   (≥0.75 warning, ≥0.95 danger), so a login near its weekly cap says so
   before the run that would hit it is started. */
const LIMITS = {
  fiveHour: 0.42,
  week: 0.81,
  model: { label: `Fable`, used: 0.96 },
}

const OPTIONS = [
  { key: `claude:system`, agent: `claude`, email: `danny@yourev.at`, limits: LIMITS },
  { key: `codex:system`, agent: `codex`, email: `ops@yourev.at` },
  {
    key: `claude:alt`,
    agent: `claude`,
    email: `alt@yourev.at`,
    hint: `Needs re-login`,
    disabled: true,
  },
]

export const entry: StyleguideEntry = {
  id: `picker-account`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Account picker`,
  blurb: `EXP-872: ONE picker for the agent AND its login — there is no separate agent pick. The list is every signed-in login the machine reports across both agents, each row the agent's brand mark beside the login's EMAIL (never the profile name, never the word "default": the device default is simply the first row), and picking one implies its agent. A dead credential rides as a muted hint. EXP-992 adds the rate-limit preview — three small bars (5h / week / the model window) off the option's fractions, right of the row on a pointer, inline under the email in the sheet.`,
  status: typedPickerStatus(`AccountPicker`, {
    web: `account-picker.tsx`,
    desktop: `account_picker.rs`,
    ios: `SharedAccountPicker.swift`,
    android: `AccountPicker.kt`,
  }),
  island: () => (
    <div className="grid gap-4">
      {/* The two trigger arms the launch surfaces use: one word of the
          composer's muted options line, and the glass form ladder's row. */}
      <div className="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
        <span>on</span>
        <AccountPicker value="claude:system" options={OPTIONS} onChange={noop} />
      </div>
      <div className="w-[18rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
        <AccountPicker
          value="claude:system"
          options={OPTIONS}
          onChange={noop}
          variant="row"
          mobileTitle="Account"
        />
      </div>
      {/* The preview a row carries, at the size it draws inside the surface. */}
      <div className="flex items-center gap-3">
        <PickerTrigger variant="pill" label="Account" value="danny@yourev.at" />
        <AccountLimitBars limits={LIMITS} />
      </div>
    </div>
  ),
}
