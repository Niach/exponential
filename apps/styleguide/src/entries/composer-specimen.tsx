import { contract } from "@exp/domain-contract"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import type { ComposerReplies as ComposerRepliesModel } from "@exp/domain-contract/session-tree"
import type { IconConcept } from "@exp/icons"
import {
  AccountPicker,
  Composer,
  ComposerEditTools,
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
// @exp/ui halves the app composes (`ComposerFrame`, `ComposerOptionsRow`,
// `ComposerEditTools` around the `Composer` card). Shared by the
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

/** EXP-1252 `MOVING_LABEL` (apps/web/src/lib/session-device-move.ts ×4). */
const movingLabel = (device: string) => `Moving to ${device}…`

/** The options line: before a start device · account · model · Plan ·
 *  repository; in a session device · model · Plan … the context ring.
 *  EXP-1252: the session's Device pin is THE launch pin (a pick moves the
 *  session); `moving` = a move is pending, the pin names the target. */
export function ComposerOptionsSpecimen({
  phase,
  moving = false,
}: {
  phase: ComposerPhase
  moving?: boolean
}) {
  const target = DEVICES[1]!
  return (
    <ComposerOptionsRow>
      <DevicePicker
        triggerVariant="inline"
        mobileTitle={phase === `start` ? `Device` : `Move to device`}
        devices={DEVICES}
        value="mint"
        onChange={noop}
        width="sm"
        trigger={
          moving ? (
            <PickerTrigger
              variant="inline"
              label="Move to device"
              value={movingLabel(target.name)}
              icon={getDeviceIcon(target)}
            />
          ) : undefined
        }
      />
      {phase === `start` && (
        <AccountPicker variant="inline" value="claude:1" options={ACCOUNTS} onChange={noop} />
      )}
      <ModelPicker models={MODELS} value={phase === `idle` ? `haiku` : `opus`} onChange={noop} />
      <Label className="gap-1.5 font-normal">
        <span>Plan</span>
        <Switch size="sm" checked={phase === `running`} aria-label="Plan mode" />
      </Label>
      {phase === `start` ? (
        <RepositoryPicker
          triggerVariant="inline"
          width="sm"
          repositories={REPOS}
          value="exp"
          onChange={noop}
        />
      ) : (
        <span className="ml-auto flex items-center">
          <ContextRing percent={phase === `running` ? 42 : 12} tone="normal" />
        </span>
      )}
    </ComposerOptionsRow>
  )
}

/** The whole composer: the card (field, "+" + inline tools, Send ⇄ Stop),
 *  the options line under it. */
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
      options={<ComposerOptionsSpecimen phase={phase} />}
      replies={replies && <ComposerReplies replies={replies} onSend={() => {}} onDismiss={() => {}} />}
    >
      <Composer
        tools={
          <>
            <ComposerTool aria-label={composerMenu.plusLabel} title={composerMenu.plusLabel}>
              <UiAddIcon />
            </ComposerTool>
            <ComposerEditTools />
          </>
        }
        submit={
          <ComposerSubmit
            stop={stop}
            aria-label={stop ? `Stop` : (submitLabel ?? `Send`)}
            title={stop ? `Stop` : (submitLabel ?? `Send`)}
          />
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
 *  minus Subagents, which `sessions.send` does not carry). In a session the
 *  subject rows (Implement issue ›, Run action ›) go: they exist only
 *  before a start. */
export function composerMenuEntries(phase: ComposerPhase): MenuEntry[] {
  const subjectRows = new Set([`implement-issue`, `run-action`])
  return composerMenu.rows
    .filter((row) => row.id !== `subagents`)
    .filter((row) => phase === `start` || !subjectRows.has(row.id ?? ``))
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
