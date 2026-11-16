// VAPP-87 + round 1: icons. The catalog's `Icon` names come from the host's
// registry (`host.icons`, the Exponential app passes its generated Lucide
// map); the renderer's OWN parts draw the glyph `core.catalog.json`
// `builtinIcons` names (`BuiltinIcon` in natives/shared.tsx), resolved
// through the host first and, without a registry, through the fallbacks
// below: the SAME Lucide glyphs `packages/icons/icons.json` maps those
// concept names to, so a host without a registry still gets working,
// matching controls. An unknown name paints the placeholder glyph and names
// itself in `data-icon`.

import { useId, type ComponentType, type SVGProps } from "react"
import {
  ArrowLeft,
  AudioLines,
  Calendar,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
  CircleArrowUp,
  CircleCheck,
  CircleDashed,
  CircleStop,
  CircleX,
  Clock,
  Copy,
  Ear,
  Ellipsis,
  File,
  Hand,
  Image as ImageGlyph,
  Info,
  LoaderCircle,
  Maximize2,
  Menu,
  Mic,
  MicOff,
  Minus,
  Paperclip,
  PhoneIncoming,
  PhoneOff,
  Play,
  Pause,
  Plus,
  QrCode,
  ScanLine,
  Search,
  Send,
  Square,
  Star,
  TriangleAlert,
  Upload,
  Volume2,
  X,
  createLucideIcon,
} from "lucide-react"
import type { IconNode } from "lucide-react"
import { CUSTOM_ICON_NODES, lookupIcon } from "@exponential_app/eui/web"
import type { GlyphGradient } from "@exponential_app/eui"
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
  pause: Pause,
  attach: Paperclip,
  spinner: LoaderCircle,
  placeholder: CircleDashed,
  /** A ZoomableImage whose picture has not loaded (pending or failed). */
  image: ImageGlyph,
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
  "ui-clear": CircleX,
  "ui-stop": CircleStop,
  "ui-attach": Paperclip,
  "ui-send": Send,
  // The app Composer's submit (icons.json `ui-submit`: circle-arrow-up).
  "ui-submit": CircleArrowUp,
  "ui-back": ArrowLeft,
  "ui-more": Ellipsis,
  "ui-menu": Menu,
  "ui-search": Search,
  "ui-icon-placeholder": CircleDashed,
  // VAPP-162: the round-5 pieces (MessageBubble, CallSurface, QrScanner, the hold button).
  "ui-read": CheckCheck,
  "ui-scan": ScanLine,
  "ui-qr-code": QrCode,
  "ui-hold": Hand,
  // EXP-1586: the ZoomableImage pill's fit button.
  "ui-fullscreen": Maximize2,
  "voice-level": AudioLines,
  "call-mute": Mic,
  "call-unmute": MicOff,
  "call-speaker": Volume2,
  "call-speaker-off": Ear,
  "call-end": PhoneOff,
  "call-incoming": PhoneIncoming,
  calendar: Calendar,
  search: Search,
  upload: Upload,
  star: Star,
  // VAPP-163: the icons.json names a Lucide registry has no glyph for (the
  // OS marks, the progress pies), so an `Icon` paints them without a host map.
  ...Object.fromEntries(Object.entries(CUSTOM_ICON_NODES).map(([name, node]) => [name, createLucideIcon(name, node as IconNode)])),
}

export function resolveIcon(icons: IconMap | undefined, name: string): IconComponent | undefined {
  return lookupIcon(icons, name, FALLBACK_ICONS)
}

export function IconGlyph({ icons, name, size, className, ...rest }: { icons?: IconMap; name: string; size?: number; className?: string } & SVGProps<SVGSVGElement>) {
  const resolved = resolveIcon(icons, name)
  const Comp: ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }> = resolved ?? CircleDashed
  return <Comp aria-hidden="true" data-icon={name} data-icon-missing={resolved ? undefined : `true`} size={size} className={className} {...rest} />
}

/** #EXP-1876: a glyph painted with a recipe's sheen (`backgroundClip:
 *  text`, `glyphGradient`): the glyph as an SVG mask over a rect filled with
 *  the gradient, so any registry icon (fill or stroke, `currentColor`) takes
 *  it. Fills its part box. */
export function GradientGlyph({ icons, name, gradient }: { icons?: IconMap; name: string; gradient: GlyphGradient }) {
  const id = useId().replace(/:/g, ``)
  return (
    <svg aria-hidden="true" data-icon={name} data-glyph-gradient="" width="100%" height="100%" style={{ display: `block`, overflow: `visible` }}>
      <defs>
        <linearGradient id={`${id}g`} x1={gradient.x1} y1={gradient.y1} x2={gradient.x2} y2={gradient.y2}>
          {gradient.stops.map((stop, i) => (
            <stop key={i} offset={stop.offset} stopColor={stop.color} />
          ))}
        </linearGradient>
        <mask id={`${id}m`} maskUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
          <g style={{ color: `#ffffff` }}>
            <IconGlyph icons={icons} name={name} width="100%" height="100%" />
          </g>
        </mask>
      </defs>
      <rect width="100%" height="100%" fill={`url(#${id}g)`} mask={`url(#${id}m)`} />
    </svg>
  )
}
