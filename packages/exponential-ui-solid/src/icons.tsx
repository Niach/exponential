// FEED-128: icons on Solid (React's icons.tsx). The catalog's `Icon` names
// come from the host's registry (`host.icons`); the renderer's OWN parts
// draw the glyph `core.catalog.json` `builtinIcons` names (`BuiltinIcon` in
// natives/shared.tsx), through the host first and then the fallbacks below:
// the SAME Lucide glyphs, by the SAME names, as the React renderer.
//
// THE INVARIANT: an icon paints the `<svg>` lucide-react paints. lucide-solid
// 0.544.0 carries the same SVG data as lucide-react 0.544.0 but differs in
// its wrapper: it adds a `lucide-icon` class, renders each child's `key` as
// an attribute, keeps `width`/`height` when they are passed undefined, and
// never adds lucide-react's default `aria-hidden="true"`. So a Lucide glyph
// is PROBED once (its children's tags + attributes, its class tokens) and
// painted by `LucideSvg`, which writes lucide-react's attribute set. A host
// icon that is not a Lucide glyph is rendered as it is.

import { For, createRoot, splitProps, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
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
} from "lucide-solid"
import { lookupIcon } from "@exponential_app/ui/web"
import type { IconComponent, IconMap } from "./host"

/** The renderer's own chrome glyphs that are NOT catalog part glyphs. */
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
 *  them, for every name the catalog's `builtinIcons` and the core macros
 *  draw (React's table, name for name). */
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
  return lookupIcon(icons, name, FALLBACK_ICONS)
}

/** A probed Lucide glyph: its class tokens (lucide-react's: `lucide-<name>`
 *  twice, deduplicated) and its children. */
interface LucideShape {
  classes: string[]
  nodes: [string, Record<string, string>][]
}

const shapes = new WeakMap<IconComponent, LucideShape | null>()

function probe(icon: IconComponent): LucideShape | null {
  if (shapes.has(icon)) return shapes.get(icon)!
  let shape: LucideShape | null = null
  if (typeof document !== `undefined`) {
    try {
      createRoot((dispose) => {
        const el = icon({}) as unknown
        if (el instanceof Element && el.localName === `svg` && el.classList.contains(`lucide`)) {
          const classes = Array.from(el.classList).filter((c) => c !== `lucide` && c !== `lucide-icon`)
          const nodes = Array.from(el.children).map((child): [string, Record<string, string>] => {
            const attrs: Record<string, string> = {}
            for (const a of Array.from(child.attributes)) if (a.name !== `key`) attrs[a.name] = a.value
            return [child.localName, attrs]
          })
          shape = { classes, nodes }
        }
        dispose()
      })
    } catch {
      shape = null
    }
  }
  shapes.set(icon, shape)
  return shape
}

const mergeClasses = (...classes: (string | undefined)[]): string => classes.filter((c, i, all): c is string => Boolean(c) && c!.trim() !== `` && all.indexOf(c) === i).join(` `).trim()

export type LucideSvgProps = JSX.SvgSVGAttributes<SVGSVGElement> & { shape: LucideShape; size?: number | string; class?: string; "data-icon"?: string; "data-icon-missing"?: string; "data-slot"?: string }

/** lucide-react's `<svg>` for a probed glyph: its defaults (`width`/`height`
 *  = `size` ?? 24 unless passed explicitly, even undefined), `class="lucide
 *  lucide-<name> <class>"`, `aria-hidden="true"` unless an a11y prop is
 *  passed, the children without their `key`. */
function LucideSvg(props: LucideSvgProps) {
  const [local, rest] = splitProps(props, [`shape`, `size`, `class`])
  const hasA11y = () => Object.keys(rest).some((k) => k.startsWith(`aria-`) || k === `role` || k === `title`)
  const size = () => local.size ?? 24
  // React spreads `rest` last: a key present in it wins even when undefined.
  const attr = (name: string, fallback: unknown) => (name in rest ? (rest as Record<string, unknown>)[name] : fallback)
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden={hasA11y() ? undefined : `true`}
      {...rest}
      width={attr(`width`, size()) as string | undefined}
      height={attr(`height`, size()) as string | undefined}
      class={mergeClasses(`lucide`, mergeClasses(...local.shape.classes, local.class))}
    >
      <For each={local.shape.nodes}>{([tag, attrs]) => <Dynamic component={tag} {...attrs} />}</For>
    </svg>
  )
}

/** One icon as lucide-react paints it (a Lucide glyph through `LucideSvg`,
 *  any other host component as it is). */
export function PaintIcon(props: { icon: IconComponent } & JSX.SvgSVGAttributes<SVGSVGElement> & { size?: number | string; class?: string; [data: `data-${string}`]: string | undefined }) {
  const [local, rest] = splitProps(props, [`icon`])
  return (
    <Dynamic
      component={(() => {
        const icon = local.icon
        const shape = probe(icon)
        return shape ? (p: Omit<LucideSvgProps, `shape`>) => <LucideSvg shape={shape} {...p} /> : icon
      })()}
      {...rest}
    />
  )
}

export function IconGlyph(props: { icons?: IconMap; name: string; size?: number; class?: string } & JSX.SvgSVGAttributes<SVGSVGElement> & { [data: `data-${string}`]: string | undefined }) {
  const [local, rest] = splitProps(props, [`icons`, `name`, `size`, `class`])
  const resolved = () => resolveIcon(local.icons, local.name)
  return (
    <PaintIcon
      icon={resolved() ?? CircleDashed}
      aria-hidden="true"
      data-icon={local.name}
      data-icon-missing={resolved() ? undefined : `true`}
      size={local.size}
      class={local.class}
      {...rest}
    />
  )
}
