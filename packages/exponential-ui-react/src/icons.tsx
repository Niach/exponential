// VAPP-87: icons. The catalog's `Icon` names come from the host's registry
// (`host.icons`, the Exponential app passes its generated Lucide map); the
// renderer's OWN chrome (a chevron on a select, the check in a checkbox)
// imports the few Lucide glyphs it needs directly, so a host without a
// registry still gets working controls. An unknown name paints the
// placeholder glyph and names itself in `data-icon`.

import type { ComponentType, SVGProps } from "react"
import { Check, ChevronDown, ChevronLeft, ChevronRight, Circle, Loader, Paperclip, Play, Search, SendHorizontal, Square, X, Calendar } from "lucide-react"
import type { IconComponent, IconMap } from "./host"

export const CHROME = {
  check: Check,
  chevronDown: ChevronDown,
  chevronLeft: ChevronLeft,
  chevronRight: ChevronRight,
  close: X,
  search: Search,
  calendar: Calendar,
  send: SendHorizontal,
  stop: Square,
  play: Play,
  attach: Paperclip,
  spinner: Loader,
  placeholder: Circle,
} as const

/** The few registry concept names the core macros use for their own
 *  chrome, so they resolve without a host registry. */
const BUILTIN: Record<string, IconComponent> = {
  "ui-check": Check,
  "ui-chevron-down": ChevronDown,
  "ui-chevron-left": ChevronLeft,
  "ui-chevron-right": ChevronRight,
  "ui-close": X,
  "ui-search": Search,
  "ui-send": SendHorizontal,
  "ui-icon-placeholder": Circle,
}

export function resolveIcon(icons: IconMap | undefined, name: string): IconComponent | undefined {
  if (!icons) return BUILTIN[name]
  const hit = typeof icons === `function` ? icons(name) : icons[name]
  return hit ?? BUILTIN[name]
}

export function IconGlyph({ icons, name, size, className, ...rest }: { icons?: IconMap; name: string; size?: number; className?: string } & SVGProps<SVGSVGElement>) {
  const Comp: ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }> = resolveIcon(icons, name) ?? Circle
  const known = resolveIcon(icons, name) !== undefined
  return <Comp aria-hidden="true" data-icon={name} data-icon-missing={known ? undefined : `true`} size={size} className={className} {...rest} />
}
