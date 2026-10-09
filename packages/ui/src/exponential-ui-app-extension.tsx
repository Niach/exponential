// VAPP-91: the Exponential APP extension (exponential-ui/extension.json) as a
// React extension: the app's own components paint its natives inside an
// Exponential UI surface, registered through the SDK's public API like any
// third party's. VAPP-102: IssueRow, IssueChip, IssueGroupBand and
// GuideSection are MACROS over the catalog's Row/Section/Chip (the core
// paints them; only the StatusGlyph leaf is ours); the row natives map 1:1
// onto the app's components. Natives without a painter yet (ComposerHeadline,
// DecisionCard, ChoiceDialog, ResultTile, Diff) render the renderer's
// extension note; VAPP-83 adds them as the dogfood screens need them.

import { defineExtension, type ExtensionDef } from "@exponential-at/ui"
import { defineReactExtension, type ExtensionComponentProps } from "@exponential-at/ui-react"
import type { IconConcept, IconName } from "@exp/icons"
import { SEMANTIC_ICONS } from "@exp/icons"
import extensionJson from "../exponential-ui/extension.json"
import devicesPackageJson from "../exponential-ui/templates/devices.json"
import { ICON_COMPONENTS } from "./icons.generated"
import type { RunMarkState } from "./agent-brand-mark"
import { DiffCounts, DiffPath, DiffStatusLetter } from "./diff-counts"
import { ListRow } from "./glass-rows"
import { LIVE_DOT_TONE, LiveDot, type LiveDotTone } from "./live-dot"
import { PrRow, type PrNodeState } from "./pr-row"
import type { TreeGuide } from "./tree-guides"
import { createContext, useContext, useMemo } from "react"

/** The core's filled guide on a depth-bearing extension row (`guide` = `{elbowAt?, tee, passThrough}`). */
function guideOf(value: unknown): TreeGuide | null {
  if (!value || typeof value !== `object`) return null
  const g = value as { elbowAt?: unknown; tee?: unknown; passThrough?: unknown }
  return { elbowAt: typeof g.elbowAt === `number` ? g.elbowAt : null, tee: g.tee === true, passThrough: Array.isArray(g.passThrough) ? g.passThrough.filter((v): v is number => typeof v === `number`) : [] }
}

const StackWordContext = createContext<{ word: string | null; topId: string | null } | null>(null)
import { RunStatusRow, type RunStatusRowTone } from "./run-status-row"
import { SessionRow } from "./session-row"
import { StatusGlyph } from "./status-glyph"

/** The app extension catalog, validated. */
export const appExtensionCatalog: ExtensionDef = defineExtension(extensionJson as unknown as ExtensionDef)

/** The declarative packages the app ships (VAPP-83's Devices template). */
export const devicesPackage = devicesPackageJson

const iconOf = (name: unknown): IconName | undefined => {
  if (typeof name !== `string`) return undefined
  const concept = SEMANTIC_ICONS[name as IconConcept] as IconName | undefined
  const resolved = (concept ?? name) as IconName
  return resolved in ICON_COMPONENTS ? resolved : undefined
}

const statusOf = (value: unknown) => {
  const s = (value ?? {}) as { icon?: string; color?: string }
  return { icon: iconOf(s.icon) ?? (`circle` as IconName), colorHex: s.color }
}

function LiveDotPainter({ props, rootProps }: ExtensionComponentProps) {
  const tone = (typeof props.tone === `string` && props.tone in LIVE_DOT_TONE ? props.tone : `muted`) as LiveDotTone
  return (
    <span {...rootProps} style={{ alignItems: `center`, justifyContent: `center` }}>
      <LiveDot tone={tone} ping={props.ping === true} label={typeof props.label === `string` ? props.label : undefined} />
    </span>
  )
}

const DISC_TONE: Record<string, string> = { primary: `primary`, success: `success`, danger: `destructive`, muted: `mutedForeground` }

function IconDiscPainter({ props, rootProps, theme, mode }: ExtensionComponentProps) {
  const icon = iconOf(props.icon)
  const Icon = icon ? ICON_COMPONENTS[icon] : undefined
  const colors = theme.modes[mode].color as Record<string, string>
  const color = colors[DISC_TONE[String(props.tone ?? `primary`)] ?? `primary`] ?? colors.primary
  // The disc = the tone at 15% (theme colours are #rrggbb[aa]).
  const disc = `${color.slice(0, 7)}26`
  return (
    <span {...rootProps} style={{ width: 32, height: 32, flexShrink: 0, alignItems: `center`, justifyContent: `center`, borderRadius: 9999, backgroundColor: disc, color }}>
      {Icon ? <Icon width={16} height={16} aria-hidden /> : null}
    </span>
  )
}

function StatusGlyphPainter({ props, rootProps }: ExtensionComponentProps) {
  return (
    <span {...rootProps} style={{ alignItems: `center` }}>
      <StatusGlyph {...statusOf(props.status)} className={props.size === `sm` ? `size-3.5` : `size-4`} />
    </span>
  )
}

const RUN_TONES = [`muted`, `amber`, `emerald`, `sky`] as const
const MARK_STATES = [`needs_input`, `working`, `review`, `done`, `ended`] as const
const PR_STATES = [`open`, `current`, `base`] as const

const oneOf = <T extends string>(values: readonly T[], value: unknown, fallback: T): T =>
  values.includes(value as T) ? (value as T) : fallback
const textOf = (value: unknown): string | null => (typeof value === `string` && value !== `` ? value : null)
const numberOf = (value: unknown): number => (typeof value === `number` && Number.isFinite(value) ? value : 0)
const markOf = (value: unknown): RunMarkState | undefined =>
  MARK_STATES.includes(value as RunMarkState) ? (value as RunMarkState) : undefined
/** The node routes a press (so the row reads as a control only then). */
const pressable = (node: ExtensionComponentProps[`node`]) => node.on?.press !== undefined

function RunStatusRowPainter({ props, rootProps }: ExtensionComponentProps) {
  return (
    <div {...rootProps}>
      <RunStatusRow
        agent={textOf(props.agent)}
        markState={markOf(props.markState)}
        caption={String(props.caption ?? ``)}
        tone={oneOf<RunStatusRowTone>(RUN_TONES, props.tone, `muted`)}
        toolLine={textOf(props.toolLine)}
        showWork={props.showWork === true}
        className="w-full"
      />
    </div>
  )
}

function SessionRowPainter({ node, props, rootProps, emit }: ExtensionComponentProps) {
  const icon = iconOf(props.deviceIcon)
  return (
    <div {...rootProps}>
      <SessionRow
        size={props.size === `small` ? `small` : `big`}
        agent={textOf(props.agent)}
        markState={markOf(props.markState)}
        identifier={textOf(props.identifier)}
        title={String(props.title ?? ``)}
        caption={textOf(props.caption)}
        captionTone={oneOf(RUN_TONES, props.captionTone, `muted`)}
        depth={numberOf(props.depth)}
        guide={guideOf(props.guide)}
        deviceIcon={icon ? ICON_COMPONENTS[icon] : undefined}
        deviceName={textOf(props.device)}
        onClick={pressable(node) ? () => emit(`press`) : undefined}
        className="w-full"
      />
    </div>
  )
}

function PrRowPainter({ node, props, rootProps, emit }: ExtensionComponentProps) {
  const rail = (props.rail ?? null) as { above?: boolean; below?: boolean } | null
  const stack = useContext(StackWordContext)
  const word = textOf(props.word) ?? (stack && stack.topId === node.id ? stack.word : null)
  return (
    <div {...rootProps}>
      <PrRow
        node={oneOf<PrNodeState>(PR_STATES, props.state, `open`)}
        identifier={textOf(props.identifier)}
        title={String(props.title ?? ``)}
        word={word}
        depth={numberOf(props.depth)}
        guide={guideOf(props.guide)}
        rail={rail}
        active={props.active === true}
        onClick={pressable(node) ? () => emit(`press`) : undefined}
        className="w-full"
      />
    </div>
  )
}

/** The member PrRows are the node's children (painted by the renderer, each
 *  with its own `rail`); the rail ends on the base-branch row, as the app's
 *  `StackRail` draws it. */
function StackRailPainter({ node, props, rootProps, children }: ExtensionComponentProps) {
  // The extension's `word` = the quiet word of the TOP row when that PrRow names none.
  const stack = useMemo(() => ({ word: textOf(props.word), topId: node.children[0]?.id ?? null }), [props.word, node.children[0]?.id])
  return (
    <StackWordContext.Provider value={stack}>
      <div {...rootProps} data-slot="stack-rail" style={{ display: `flex`, flexDirection: `column` }}>
        {children}
        <PrRow node="base" title={String(props.baseBranch ?? ``)} rail={{ above: node.children.length > 0 }} className="w-full" />
      </div>
    </StackWordContext.Provider>
  )
}

function DiffCountsPainter({ props, rootProps }: ExtensionComponentProps) {
  return (
    <span {...rootProps} style={{ alignItems: `center` }}>
      <DiffCounts additions={numberOf(props.additions)} deletions={numberOf(props.deletions)} className="text-xs" />
    </span>
  )
}

/** The catalog spells a removed file `deleted`; the app's diff contract `removed`. */
const DIFF_STATUS = { added: `added`, modified: `modified`, deleted: `removed`, renamed: `renamed` } as const

function DiffFileRowPainter({ node, props, rootProps, emit }: ExtensionComponentProps) {
  const status = DIFF_STATUS[props.status as keyof typeof DIFF_STATUS] ?? `modified`
  const clickable = pressable(node)
  return (
    <div {...rootProps}>
      <ListRow
        interactive={clickable}
        onClick={clickable ? () => emit(`press`) : undefined}
        data-diff-file-row={status}
        className="w-full gap-2 py-1.5 text-xs"
      >
        <DiffStatusLetter status={status} />
        <DiffPath path={String(props.path ?? ``)} className="flex-1" />
        <DiffCounts additions={numberOf(props.additions)} deletions={numberOf(props.deletions)} />
      </ListRow>
    </div>
  )
}

/** Register with `<ExponentialSurface extensions={[appReactExtension]}>`
 *  or the host's `registerExtension(appExtensionCatalog)`. */
export const appReactExtension = defineReactExtension({
  catalog: appExtensionCatalog,
  components: {
    LiveDot: LiveDotPainter,
    IconDisc: IconDiscPainter,
    StatusGlyph: StatusGlyphPainter,
    RunStatusRow: RunStatusRowPainter,
    SessionRow: SessionRowPainter,
    PrRow: PrRowPainter,
    StackRail: StackRailPainter,
    DiffCounts: DiffCountsPainter,
    DiffFileRow: DiffFileRowPainter,
  },
})
