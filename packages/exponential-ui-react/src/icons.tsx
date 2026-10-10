// VAPP-87 + round 1: icons. The catalog's `Icon` names come from the host's
// registry (`host.icons`, the Exponential app passes its generated Lucide
// map); the renderer's OWN parts draw the glyph `core.catalog.json`
// `builtinIcons` names (`BuiltinIcon` in natives/shared.tsx), resolved
// through the host first and, without a registry, through the fallbacks
// below: the SAME Lucide glyphs `packages/icons/icons.json` maps those
// concept names to, so a host without a registry still gets working,
// matching controls. An unknown name paints the placeholder glyph and names
// itself in `data-icon`.

import type { ComponentType, SVGProps } from "react"
import {
  ArrowLeft,
  Calendar,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
  CircleCheck,
  CircleDashed,
  CircleStop,
  CircleX,
  Clock,
  Copy,
  Ellipsis,
  File,
  Info,
  Loader,
  Menu,
  Minus,
  Paperclip,
  Play,
  Plus,
  Search,
  Send,
  Square,
  Star,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react"
import type { IconComponent, IconMap } from "./host"

/** The renderer's own chrome glyphs that are NOT catalog part glyphs (the
 *  busy spinner, media play, the field and picker adornments). */
export const CHROME = {
  check: Check,
  chevronDown: ChevronDown,
  chevronUp: ChevronUp,
  chevronLeft: ChevronLeft,
  chevronRight: ChevronRight,
  close: X,
  search: Search,
  calendar: Calendar,
  send: Send,
  stop: Square,
  play: Play,
  attach: Paperclip,
  spinner: Loader,
  placeholder: CircleDashed,
} as const

/** icons.json concept / registry names → the Lucide glyph icons.json gives
 *  them (`semantic`), for every name the catalog's `builtinIcons` and the
 *  core macros draw. */
export const FALLBACK_ICONS: Record<string, IconComponent> = {
  "ui-check": Check,
  "ui-chevron-down": ChevronDown,
  "ui-chevron-up": ChevronUp,
  "ui-chevron-left": ChevronLeft,
  "ui-chevron-right": ChevronRight,
  "ui-selector": ChevronsUpDown,
  "ui-close": X,
  "ui-copy": Copy,
  "ui-file": File,
  "ui-minus": Minus,
  "ui-add": Plus,
  "ui-clock": Clock,
  "ui-info": Info,
  "ui-success": CircleCheck,
  "ui-warning": TriangleAlert,
  "ui-error": CircleX,
  "ui-stop": CircleStop,
  "ui-attach": Paperclip,
  "ui-send": Send,
  "ui-back": ArrowLeft,
  "ui-more": Ellipsis,
  "ui-menu": Menu,
  "ui-search": Search,
  "ui-icon-placeholder": CircleDashed,
  calendar: Calendar,
  search: Search,
  upload: Upload,
  star: Star,
}

export function resolveIcon(icons: IconMap | undefined, name: string): IconComponent | undefined {
  if (!icons) return FALLBACK_ICONS[name]
  const hit = typeof icons === `function` ? icons(name) : icons[name]
  return hit ?? FALLBACK_ICONS[name]
}

export function IconGlyph({ icons, name, size, className, ...rest }: { icons?: IconMap; name: string; size?: number; className?: string } & SVGProps<SVGSVGElement>) {
  const resolved = resolveIcon(icons, name)
  const Comp: ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }> = resolved ?? CircleDashed
  return <Comp aria-hidden="true" data-icon={name} data-icon-missing={resolved ? undefined : `true`} size={size} className={className} {...rest} />
}
