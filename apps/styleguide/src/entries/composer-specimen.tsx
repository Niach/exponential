import { contract } from "@exp/domain-contract"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import type { ComposerReplies as ComposerRepliesModel } from "@exp/domain-contract/session-tree"
import type { IconConcept } from "@exp/icons"
import {
  AccountPicker,
  Composer,
  ComposerFrame,
  ComposerOptionsRow,
  ComposerReplies,
  ComposerSubmit,
  ComposerTool,
  ContextRing,
  DevicePicker,
  Label,
  ModelPicker,
  PickerTrigger,
  RepositoryPicker,
  Switch,
  conceptIcon,
  getDeviceIcon,
  type MenuEntry,
} from "@exp/ui"

// EXP-1334: THE one composer (start · thread · session) drawn from the real
// @exp/ui halves the app composes (`ComposerFrame`, `ComposerOptionsRow`
// around the `Composer` card). N-12 (the iOS row ×4): the "+" is the only
// tool; in a session the Device pin (its glyph alone) · the model pin · the
// Plan switch sit IN the card's tool row and the context ring beside Send ⇄
// Stop; the options line under the card is the start's alone. Shared by the
// composer-options and composer-dialog entries; not an entry itself.

const noop = (): void => {}
const UiAddIcon = conceptIcon(`ui-add`)

export type ComposerPhase = `start` | `running` | `idle`

const DEVICES = [
  { id: `mint`, name: `mint · This device`, kind: `desktop` },
  { id: `buildbox`, name: `buildbox`, kind: `server` },
]
const ACCOUNTS = [
  { key: `claude:1`, agent: `claude`, email: `danny@example.dev` },
  { key: `codex:1`, agent: `codex`, email: `danny@example.dev` },
]
const REPOS = [
  { id: `exp`, fullName: `Niach/exponential` },
  { id: `site`, fullName: `Niach/website` },
]
/** THE model choices (contract `codingModel`, no Default row). */
export const MODELS = contract.codingModel.values.map((value) => ({
  value,
  label: value.charAt(0).toUpperCase() + value.slice(1),
}))

/** N-12: the session composer's in-card row: the Device pin as its glyph
 *  alone (EXP-1252: THE launch pin; a pick moves the session) · the model
 *  pin · the Plan switch. */
export function ComposerSessionRowSpecimen({
  phase,
}: {
  phase: Exclude<ComposerPhase, `start`>
}) {
  const current = DEVICES[0]!
  return (
    <div className="flex min-w-0 items-center gap-x-2.5 pl-1 text-xs text-muted-foreground">
      <DevicePicker
        triggerVariant="inline"
        mobileTitle="Move to device"
        devices={DEVICES}
        value="mint"
        onChange={noop}
        width="sm"
        trigger={
          <PickerTrigger
            variant="inline"
            iconOnly
            label="Move to device"
            value={current.name}
            icon={getDeviceIcon(current)}
          />
        }
      />
      <ModelPicker models={MODELS} value={phase === `idle` ? `haiku` : `opus`} onChange={noop} />
      <Label className="gap-1.5 font-normal">
        <span>Plan</span>
        <Switch size="sm" checked={phase === `running`} aria-label="Plan mode" />
      </Label>
    </div>
  )
}

/** The options line under the START composer: device · account · model ·
 *  Plan · repository. (A session's pins sit in the card: above.) */
export function ComposerOptionsSpecimen({ phase }: { phase: ComposerPhase }) {
  return (
    <ComposerOptionsRow>
      <DevicePicker
        triggerVariant="inline"
        mobileTitle={phase === `start` ? `Device` : `Move to device`}
        devices={DEVICES}
        value="mint"
        onChange={noop}
        width="sm"
      />
      {phase === `start` && (
        <AccountPicker variant="inline" value="claude:1" options={ACCOUNTS} onChange={noop} />
      )}
      <ModelPicker models={MODELS} value={phase === `idle` ? `haiku` : `opus`} onChange={noop} />
      <Label className="gap-1.5 font-normal">
        <span>Plan</span>
        <Switch size="sm" checked={phase === `running`} aria-label="Plan mode" />
      </Label>
      {phase === `start` && (
        <RepositoryPicker
          triggerVariant="inline"
          width="sm"
          repositories={REPOS}
          value="exp"
          onChange={noop}
        />
      )}
    </ComposerOptionsRow>
  )
}

/** The whole composer: the card (the field, the "+", in a session the pins
 *  row and the context ring beside Send ⇄ Stop); before a start the
 *  options line under it. */
export function ComposerSpecimen({
  phase,
  placeholder,
  submitLabel,
  replies,
}: {
  phase: ComposerPhase
  placeholder: string
  submitLabel?: string
  /** EXP-1211: the quick replies above the card. */
  replies?: ComposerRepliesModel
}) {
  const stop = phase === `running`
  return (
    <ComposerFrame
      className="w-[36rem] max-w-full"
      options={phase === `start` ? <ComposerOptionsSpecimen phase={phase} /> : undefined}
      replies={replies && <ComposerReplies replies={replies} onSend={() => {}} onDismiss={() => {}} />}
    >
      <Composer
        tools={
          <>
            <ComposerTool aria-label={composerMenu.plusLabel} title={composerMenu.plusLabel}>
              <UiAddIcon />
            </ComposerTool>
            {phase !== `start` && <ComposerSessionRowSpecimen phase={phase} />}
          </>
        }
        submit={
          <>
            {phase !== `start` && <ContextRing percent={phase === `running` ? 42 : 12} tone="normal" />}
            <ComposerSubmit
              stop={stop}
              aria-label={stop ? `Stop` : (submitLabel ?? `Send`)}
              title={stop ? `Stop` : (submitLabel ?? `Send`)}
            />
          </>
        }
      >
        <div className="min-h-20 px-3 pt-3 text-sm text-muted-foreground">{placeholder}</div>
      </Composer>
    </ComposerFrame>
  )
}

/** The value a row shows in the specimen (the app reads the model). */
const SAMPLE_VALUES: Record<string, string> = {
  effort: `High`,
  "mcp-servers": `2`,
}
const SAMPLE_CHECKED: Record<string, boolean> = {
  ultracode: false,
  "computer-use": true,
}

/** The "+" rows straight from the fixture, every condition met (EXP-1293:
 *  minus Subagents, which `sessions.send` does not carry), on the phase's
 *  surface (S-3: launch | session; a row's `surfaces`), separators tidied. */
export function composerMenuEntries(phase: ComposerPhase): MenuEntry[] {
  const surface = phase === `start` ? `launch` : `session`
  const rows = composerMenu.rows
    .filter((row) => row.id !== `subagents`)
    .filter((row) => row.kind === `separator` || (`surfaces` in row && (row.surfaces ?? []).includes(surface)))
    .filter((row, index, all) => row.kind !== `separator` || (index > 0 && all[index - 1]!.kind !== `separator`))
  while (rows[rows.length - 1]?.kind === `separator`) rows.pop()
  return rows
    .map((row): MenuEntry => {
      if (row.kind === `separator`) return { kind: `separator` }
      const base = {
        id: row.id,
        label: row.label ?? ``,
        icon: conceptIcon(row.icon as IconConcept),
      }
      if (row.kind === `toggle`) {
        return {
          ...base,
          kind: `toggle`,
          checked: SAMPLE_CHECKED[row.id ?? ``] ?? false,
          onChange: noop,
        }
      }
      if (row.kind === `submenu`) {
        return { ...base, kind: `submenu`, value: SAMPLE_VALUES[row.id ?? ``], entries: [] }
      }
      return { ...base, kind: `item`, onSelect: noop }
    })
}
