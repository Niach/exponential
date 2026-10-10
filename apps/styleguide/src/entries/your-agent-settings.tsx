import { AccountPicker, GlassGroup, Picker, PickerList, SubShellHost } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1269 DRAFT — Settings → Your agent, the member's side: ONE component
// that configures ONE thing, where the session runs. The team's agent device
// (the owner's pick in Team agents), or This computer (local only: the IDE
// the person is using, while the app is open). Only the local choice adds a
// row, the account the session is launched on (the composer's AccountPicker).
// Memory is not here: knowledge is the team's, in Postgres, shared live
// (#EXP-782), the same for every place the session runs.

const noop = (): void => {}

const LIMITS = {
  fiveHour: 0.31,
  week: 0.64,
  model: { label: `Haiku`, used: 0.12 },
}

const ACCOUNTS = [
  { key: `claude:system`, agent: `claude`, email: `danny@yourev.at`, limits: LIMITS },
  { key: `codex:system`, agent: `codex`, email: `ops@yourev.at` },
]

const WHERE = [
  {
    value: `team`,
    label: `Team device`,
    description: `Studio, shared by Danny`,
  },
  {
    value: `local`,
    label: `This computer`,
    description: `Local only, while the app is open`,
  },
]

export const entry: StyleguideEntry = {
  id: `your-agent-settings`,
  section: `special`,
  owner: `EXP-1269`,
  title: `Your agent (member settings)`,
  blurb: `DRAFT for EXP-1269, every member, ONE component: "Runs on" is the only choice — the team's agent device (the owner's pick in Team agents) or This computer (local only, while the app is open). Picking This computer adds ONE row, the account the session is launched on, through the same account picker the composer uses (the login's email, never a profile name, with the rate-limit preview). On the team device there is no account row: the device owner's login pays. Nothing about memory here: knowledge is the team's and lives on the server, the same wherever the session runs. The list beside the group is the picker's own surface, the two choices as rows.`,
  status: {
    web: { state: `n/a`, note: `Draft (EXP-1269): no Your agent settings exist yet.` },
    desktop: { state: `n/a`, note: `Draft (EXP-1269): the IDE adds the local choice first (it IS the computer).` },
    ios: { state: `n/a`, note: `Draft (EXP-1269): phones offer the team device only.` },
    android: { state: `n/a`, note: `Draft (EXP-1269): phones offer the team device only.` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-4">
      <SubShellHost className="w-[22rem]">
        <GlassGroup>
          <Picker
            mode="single"
            triggerVariant="row"
            mobileTitle="Runs on"
            value="local"
            onChange={noop}
            items={WHERE}
          />
          {/* Only while "This computer" is picked: on the team device the
              DEVICE OWNER's login pays, and the row would be a lie. */}
          <AccountPicker
            value="claude:system"
            options={ACCOUNTS}
            onChange={noop}
            variant="row"
            mobileTitle="Account"
          />
        </GlassGroup>
      </SubShellHost>
      <div className="w-[18rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
        <PickerList mode="single" items={WHERE} value="local" onChange={noop} />
      </div>
    </div>
  ),
}
