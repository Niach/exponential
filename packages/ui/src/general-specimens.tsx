import { GlassGroup, GlassSectionHeader, GlassTabsRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { MentionPill } from "./mention-pill"
import { PrList, StackRail } from "./pr-row"
import { ScheduleFields, scheduleDraftFromCron } from "./schedule-fields"
import { SubShellHost } from "./sub-shell"
import { TabsTrigger } from "./tabs"

// EXP-1558: the styleguide boards of three general entries (mention-pill,
// schedule-picker, pr-row), lifted out of apps/styleguide so the app's
// `?dev=` captures render the very same specimen the natives' `sg_*` shots
// and the desktop styleguide twins draw. The styleguide entries import them.

const noop = (): void => {}

// ── mention-pill ─────────────────────────────────────────────────────────
// Thread e4a04d22: a chat line with the REAL pill for a member beside an
// unresolved `@x@y.com` that stays the written text.
export function MentionPillSpecimen() {
  return (
    <div data-testid="mention-pill-specimen" className="grid max-w-md gap-2 text-sm text-foreground">
      <p>
        Ready for review, <MentionPill name="Ada Lovelace" email="ada@example.com" />
      </p>
      <p>cc @x@y.com</p>
    </div>
  )
}

// ── schedule-picker ──────────────────────────────────────────────────────
// NEW-2 (schedule-picker.json). The capture board (`?dev=schedule-picker`)
// is the natives' and desktop's `sg_schedule-picker` line for line: the
// minutes repeat (EXP-1363: no Time zone row), then a weekly Monday–Friday
// 09:00 schedule. `full` (the styleguide entry) draws the weekly one as the
// trigger editor does, under the Schedule · On event strip, and adds a
// custom cron the rows cannot express.
const WEEKLY = scheduleDraftFromCron(`0 9 * * 1-5`, `Europe/Vienna`)
const MINUTES = scheduleDraftFromCron(`*/10 * * * *`, `Europe/Vienna`)
const CUSTOM = scheduleDraftFromCron(`*/10 * * * 1-5`, `Europe/Vienna`)

export function SchedulePickerSpecimen({ full = false }: { full?: boolean }) {
  return (
    <div data-testid="schedule-picker-specimen">
      <SubShellHost className="w-[30rem] max-w-full space-y-4">
        {full ? (
          <>
            <GlassGroup>
              <GlassTabsRow value="schedule" onValueChange={noop}>
                <TabsTrigger value="schedule">Schedule</TabsTrigger>
                <TabsTrigger value="event">On event</TabsTrigger>
              </GlassTabsRow>
              <ScheduleFields value={WEEKLY} onChange={noop} />
            </GlassGroup>
            <GlassGroup>
              <ScheduleFields value={MINUTES} onChange={noop} />
            </GlassGroup>
            <GlassGroup>
              <ScheduleFields value={CUSTOM} onChange={noop} />
            </GlassGroup>
          </>
        ) : (
          <>
            <GlassGroup>
              <ScheduleFields value={MINUTES} onChange={noop} />
            </GlassGroup>
            <GlassGroup>
              <ScheduleFields value={WEEKLY} onChange={noop} />
            </GlassGroup>
          </>
        )}
      </SubShellHost>
    </div>
  )
}

// ── pr-row ───────────────────────────────────────────────────────────────
// EXP-1248: THE PR row in its two shapes: a PR TREE nests with tree guides; a
// linear STACK hangs off one rail down to its base branch, the current member
// washed. `full` (the styleguide entry) pins a hovered member's ghost "Merge
// through here" on one row; the capture board leaves it off, like the
// natives' `sg_pr-row` (phones offer it from the long-press menu).

const ExternalIcon = conceptIcon(`ui-external-link`)

const TREE = [
  { key: `1250`, identifier: `EXP-1250`, title: `opening tabs closes all at some point` },
  { key: `1252`, identifier: `EXP-1252`, title: `tabs per team on desktop`, depth: 1 },
  { key: `1253`, identifier: `EXP-1253`, title: `inbox stepping reuses one tab`, depth: 1 },
]

const STACK = [
  { key: `100`, identifier: `VAPP-100`, title: `SwiftUI parity with the round-1/2 contract`, current: true },
  { key: `98`, identifier: `VAPP-98`, title: `Exponential UI renderer hardening round 1` },
  { key: `93`, identifier: `VAPP-93`, title: `ui.exponential.at: the Exponential UI site` },
  { key: `91`, identifier: `VAPP-91`, title: `Exponential UI host API ×4` },
]

export function PrRowSpecimen({ full = false }: { full?: boolean }) {
  return (
    <div data-testid="pr-row-specimen" className="flex w-[640px] max-w-full flex-col gap-4">
      <div>
        <GlassSectionHeader label="Exponential" />
        <PrList rows={TREE.map((row) => ({ ...row, onOpen: noop }))} />
        <StackRail
          members={STACK}
          baseBranch="master"
          word="stack"
          onOpen={noop}
          onMergeThrough={noop}
          defaultHoveredKey={full ? `93` : undefined}
        />
      </div>
      <div>
        <GlassSectionHeader label="Not linked to an issue" />
        <PrList
          rows={[
            {
              key: `1013`,
              identifier: `#1013`,
              title: `Dockerfiles: stub the exponential-ui workspace packages`,
              onOpen: noop,
              trailing: <ExternalIcon className="size-3.5 shrink-0 text-muted-foreground" />,
            },
          ]}
        />
      </div>
    </div>
  )
}
