import {
  DevicePicker,
  GlassGroup,
  GlassSectionHeader,
  GlassToggleRow,
  ListRow,
  LiveDot,
  Picker,
  PickerTrigger,
  SETTINGS_LIST_CLASS,
  SubShellHost,
  UserAvatar,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1269 DRAFT — Team settings → Team agents, the owner's section. Nothing
// is built: this entry pins the ORDER and the words before any client draws
// it. Built from the settings ladder only (EXP-1076): `GlassGroup` for the
// fields, a `GlassSectionHeader` band over flat `ListRow`s for the members.

const noop = (): void => {}

/* Only devices SHARED with the team can carry the team's agents; an unshared
   machine stays in the list, disabled, with the reason as its second line
   (the device picker's own rule). */
const DEVICES = [
  { id: `studio`, name: `Studio`, icon: `laptop` },
  { id: `builder`, name: `builder-01`, kind: `server` },
  {
    id: `macbook`,
    name: `Danny's MacBook`,
    description: `Not shared with the team`,
    disabled: true,
  },
]

const MEMBERS = [
  {
    user: { id: `u-danny`, name: `Danny Straehhuber`, email: `danny@yourev.at` },
    tone: `live` as const,
    ping: true,
    caption: `Live · in a call since 09:12`,
  },
  {
    user: { id: `u-jev`, name: `Jev Marin`, email: `jev@yourev.at` },
    tone: `idle` as const,
    ping: false,
    caption: `Idle · resumes when the app opens`,
  },
  {
    user: { id: `u-mia`, name: `Mia Ortner`, email: `mia@yourev.at` },
    tone: `muted` as const,
    ping: false,
    caption: `Off · turned off in her settings`,
  },
]

export const entry: StyleguideEntry = {
  id: `team-agents-settings`,
  section: `special`,
  owner: `EXP-1269`,
  title: `Team agents (team settings)`,
  blurb: `DRAFT for EXP-1269, owner-only. One section in Team settings: where the team's agents run (a SHARED device only; an unshared machine is listed disabled with the reason), the model, when an idle agent winds down, the hour a new day starts (the dream turn writes the diary first), memory on/off (whether the agents write facts and a daily diary into the team's knowledge, which lives on the server and syncs live, EXP-782), and the Caretaker toggle (the team-scoped agent of EXP-1173). Under it the Members band: one flat row per member with the avatar, the name, and the agent's state as a live dot and a caption (live / idle / off). No per-member controls here: a member only picks WHERE their session runs, in Your agent.`,
  status: {
    web: { state: `n/a`, note: `Draft (EXP-1269): no Team agents section exists yet.` },
    desktop: { state: `n/a`, note: `Draft (EXP-1269): the IDE's Settings → General mirrors it later.` },
    ios: { state: `n/a`, note: `Draft (EXP-1269): team settings on phones follow the web.` },
    android: { state: `n/a`, note: `Draft (EXP-1269): team settings on phones follow the web.` },
  },
  island: () => (
    <SubShellHost className="w-[26rem]">
      <GlassSectionHeader label="Team agents" />
      <GlassGroup>
        <DevicePicker
          devices={DEVICES}
          value="studio"
          onChange={noop}
          trigger={<PickerTrigger variant="row" label="Runs on" value="Studio" />}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle="Model"
          value="haiku"
          onChange={noop}
          items={[
            { value: `haiku`, label: `Haiku` },
            { value: `sonnet`, label: `Sonnet` },
            { value: `opus`, label: `Opus` },
          ]}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle="Winds down after"
          value="30"
          onChange={noop}
          items={[
            { value: `15`, label: `15 minutes` },
            { value: `30`, label: `30 minutes` },
            { value: `120`, label: `2 hours` },
          ]}
        />
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle="New day at"
          value="04:00"
          onChange={noop}
          items={[
            { value: `03:00`, label: `03:00` },
            { value: `04:00`, label: `04:00` },
            { value: `06:00`, label: `06:00` },
          ]}
        />
        <GlassToggleRow
          id="demo-team-agents-memory"
          label="Memory"
          description="Agents write facts and a diary into Team knowledge"
          checked
          onCheckedChange={noop}
        />
        <GlassToggleRow
          id="demo-team-agents-caretaker"
          label="Caretaker"
          description="Looks after every board, hourly"
          checked
          onCheckedChange={noop}
        />
      </GlassGroup>
      <GlassSectionHeader label="Members" count={MEMBERS.length} />
      <div className={SETTINGS_LIST_CLASS}>
        {MEMBERS.map((member) => (
          <ListRow key={member.user.id}>
            <UserAvatar user={member.user} size={24} />
            <span className="min-w-0 flex-1 truncate text-sm text-foreground">
              {member.user.name}
            </span>
            <LiveDot tone={member.tone} ping={member.ping} className="shrink-0" />
            <span className="shrink-0 text-xs text-muted-foreground">{member.caption}</span>
          </ListRow>
        ))}
      </div>
    </SubShellHost>
  ),
}
