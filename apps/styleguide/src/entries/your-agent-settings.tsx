import {
  AccountPicker,
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
  Picker,
  SubShellHost,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1269 DRAFT — Settings → Your agent, the member's side. Two arms of ONE
// surface: on the team device (the owner's pick in Team agents) or LOCAL ONLY
// on this computer, where the account row appears and the assistant session
// is launched on the login picked there (the same `AccountPicker` the
// composer uses). Nothing is built; the entry pins order and words.

const noop = (): void => {}
const ResetIcon = conceptIcon(`ui-delete`)

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

function Arm({ where }: { where: `team` | `local` }) {
  return (
    <SubShellHost className="w-[22rem]">
      <GlassSectionHeader label={where === `team` ? `On the team device` : `Local only`} />
      <GlassGroup>
        <GlassToggleRow
          id={`demo-your-agent-${where}-on`}
          label="Your agent"
          description="One thread that remembers, pinned in the rail"
          checked
          onCheckedChange={noop}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle="Runs on"
          value={where}
          onChange={noop}
          items={WHERE}
        />
        {/* The account row exists only on the local arm: on the team device
            the DEVICE OWNER's login pays, and the row would be a lie. */}
        {where === `local` && (
          <AccountPicker
            value="claude:system"
            options={ACCOUNTS}
            onChange={noop}
            variant="row"
            mobileTitle="Account"
          />
        )}
      </GlassGroup>
      <GlassGroup>
        <div className="flex items-center gap-3 px-4 py-3 text-sm text-destructive">
          <ResetIcon className="size-3.5" />
          Reset memory
        </div>
      </GlassGroup>
    </SubShellHost>
  )
}

export const entry: StyleguideEntry = {
  id: `your-agent-settings`,
  section: `special`,
  owner: `EXP-1269`,
  title: `Your agent (member settings)`,
  blurb: `DRAFT for EXP-1269, every member. The switch, then "Runs on": the team device the owner picked, or This computer (local only: the session lives on this IDE while the app is open and winds down with it). The local arm adds ONE row, the account the assistant session is launched on, through the same account picker the composer uses (the login's email, never a profile name, with the rate-limit preview). On the team device there is no account row: the device owner's login pays, and the Team agents section says so. "Reset memory" is a plain destructive row, the twin of "Remove device".`,
  status: {
    web: { state: `n/a`, note: `Draft (EXP-1269): no Your agent settings exist yet.` },
    desktop: { state: `n/a`, note: `Draft (EXP-1269): the IDE adds the local arm first (it IS the computer).` },
    ios: { state: `n/a`, note: `Draft (EXP-1269): phones get the team-device arm only.` },
    android: { state: `n/a`, note: `Draft (EXP-1269): phones get the team-device arm only.` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-4">
      <Arm where="team" />
      <Arm where="local" />
    </div>
  ),
}
