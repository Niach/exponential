// VAPP-91: the Exponential APP extension (exponential-ui/extension.json) as a
// React extension: the app's own components paint its natives inside an
// Exponential UI surface, registered through the SDK's public API like any
// third party's. Natives without a painter yet (IssueRow, RunRow, Diff…)
// render the renderer's extension note; VAPP-83 adds them as the dogfood
// screens need them.

import { defineExtension, type ExtensionDef } from "@exponential-at/ui"
import { defineReactExtension, type ExtensionComponentProps } from "@exponential-at/ui-react"
import type { IconConcept, IconName } from "@exp/icons"
import { SEMANTIC_ICONS } from "@exp/icons"
import extensionJson from "../exponential-ui/extension.json"
import devicesPackageJson from "../exponential-ui/templates/devices.json"
import { ICON_COMPONENTS } from "./icons.generated"
import { IssueChip } from "./issue-chip"
import { LIVE_DOT_TONE, LiveDot, type LiveDotTone } from "./live-dot"
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

function IssueChipPainter({ props, rootProps, emit }: ExtensionComponentProps) {
  return (
    <span {...rootProps}>
      <IssueChip
        identifier={String(props.identifier ?? ``)}
        title={String(props.title ?? ``)}
        status={statusOf(props.status)}
        size={props.size === `sm` ? `sm` : `md`}
        onClick={() => emit(`press`)}
        onRemove={props.removable === true ? () => emit(`remove`) : undefined}
      />
    </span>
  )
}

/** Register with `<ExponentialSurface extensions={[appReactExtension]}>`
 *  or the host's `registerExtension(appExtensionCatalog)`. */
export const appReactExtension = defineReactExtension({
  catalog: appExtensionCatalog,
  components: { LiveDot: LiveDotPainter, IconDisc: IconDiscPainter, StatusGlyph: StatusGlyphPainter, IssueChip: IssueChipPainter },
})
