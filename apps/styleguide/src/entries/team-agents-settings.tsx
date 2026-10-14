import fixture from "@exp/domain-contract/fixtures/agent-settings.json"
import { triggerSummary } from "@exp/domain-contract/agent-settings"
import {
  AccountPicker,
  BARE_FIELD_CLASS,
  Button,
  ColorPicker,
  conceptIcon,
  DevicePicker,
  GlassGroup,
  IconPicker,
  Input,
  ListRow,
  Pill,
  SubShellHost,
  Switch,
  Textarea,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2 — Team settings → Agents, ONE agent's page
// (fixtures/agent-settings.json ×4). Managed exactly like a board: the
// board page's layout, the fixture's `fields` top to bottom — identity (icon
// picker + colour + name in ONE row), Device, Account, Schedule (a row per
// trigger reading `triggerSummary`, a switch each, `+ Add`), the
// Instructions card, then the owner-only Delete. Built from the settings
// ladder only (EXP-1076).

const noop = (): void => {}
const COPY = fixture.copy
const UiAddIcon = conceptIcon(`ui-add`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

const DEVICES = [
  { id: `build`, name: `Acme build server`, kind: `server` },
  { id: `studio`, name: `Studio`, icon: `laptop` },
  { id: `macbook`, name: `Danny's MacBook`, description: `Offline`, disabled: true },
]

const ACCOUNTS = [
  {
    key: `claude:work`,
    agent: `claude`,
    email: `demo@exponential.at`,
    limits: { fiveHour: 0.31, week: 0.64 },
  },
  { key: `codex:main`, agent: `codex`, email: `ops@exponential.at` },
]

/* The fixture's own summary cases, so the rows read exactly what every
   client derives. */
const TRIGGERS = fixture.summaryCases.slice(0, 2).map((entry, index) => ({
  id: `t${index}`,
  trigger: entry.trigger,
}))

export const entry: StyleguideEntry = {
  id: `team-agents-settings`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Agent settings (team settings)`,
  blurb: `Team settings → Agents: the settings nav lists every team agent like a board (icon in its colour + name, then "New agent", owner-only) and an agent is the board page's layout. Identity is ONE row (icon picker, colour, name; name saves on blur/Enter, the pickers at once), then Device (required: the team's online machines the caller may start on) and, in a second group, Account (optional: the device's last used login; the value reads exactly like the composer's account chip, the agent's brand mark + the login's email, \`agentAccountValue\` ×4) and Schedule (one row per trigger with its switch, "+ Add" opens the trigger editor's WHEN fields; none = "${COPY.noSchedule}"). Then the Instructions card (server-only, loaded on open, saved on blur) and the owner-only destructive Delete. Members read the same page with the controls off.`,
  status: {
    web: {
      state: `ok`,
      symbol: `AgentSettingsPage`,
      file: `apps/web/src/components/team/agents-section.tsx`,
    },
    desktop: { state: `n/a`, note: `EXP-1293: the IDE's Settings → Agents lands in its own lane.` },
    ios: { state: `n/a`, note: `EXP-1293: team settings on phones follow the web.` },
    android: { state: `n/a`, note: `EXP-1293: team settings on phones follow the web.` },
  },
  island: () => (
    <SubShellHost className="w-[30rem] space-y-4">
      <GlassGroup>
        <div className="flex items-center gap-2 px-4 py-3">
          <IconPicker id="demo-agent-icon" value="bug" onChange={noop} color="#a78bfa" />
          <ColorPicker id="demo-agent-color" value="#a78bfa" onChange={noop} />
          <Input
            aria-label="Name"
            value="Triage"
            onChange={noop}
            placeholder={COPY.namePlaceholder}
            className={BARE_FIELD_CLASS}
          />
        </div>
        <DevicePicker
          triggerVariant="row"
          mobileTitle="Device"
          devices={DEVICES}
          value="build"
          onChange={noop}
        />
      </GlassGroup>
      <GlassGroup>
        <AccountPicker
          variant="row"
          mobileTitle="Account"
          value="claude:work"
          options={ACCOUNTS}
          onChange={noop}
        />
        <div className="flex items-center gap-3 px-4 py-3 text-sm">
          <span className="shrink-0 text-foreground">Schedule</span>
          <span className="ml-auto">
            <Pill mode="action" onClick={noop}>
              <UiAddIcon className="size-3" />
              {COPY.addTrigger}
            </Pill>
          </span>
        </div>
        {TRIGGERS.map(({ id, trigger }) => (
          <ListRow key={id} className="px-4">
            <span className="min-w-0 flex-1 truncate text-sm">
              {triggerSummary({ ...trigger, enabled: true })}
            </span>
            <Switch checked={trigger.enabled !== false} onCheckedChange={noop} />
          </ListRow>
        ))}
      </GlassGroup>
      <div className="space-y-1">
        <GlassGroup>
          <Textarea
            aria-label="Instructions"
            value="Look after the boards: merge duplicates, add labels and priorities, start a thread for anything that needs code."
            onChange={noop}
            placeholder={COPY.instructionsPlaceholder}
            rows={4}
            className="rounded-none border-0 bg-transparent px-4 py-3 text-sm shadow-none focus-visible:ring-0"
          />
        </GlassGroup>
        <p className="px-1.5 text-xs text-muted-foreground">Instructions</p>
      </div>
      <Button variant="destructive" size="sm" onClick={noop}>
        <UiDeleteIcon />
        Delete agent
      </Button>
    </SubShellHost>
  ),
}
