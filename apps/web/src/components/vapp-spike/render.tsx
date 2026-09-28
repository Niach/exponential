// VAPP-4 spike (throwaway): the real-CSS reference painter for the vApp
// kitchen sink. Containers are divs laid out by the browser from the SAME
// style objects the taffy core reads; leaves are real `@exp/ui` controls
// inside a leaf box. `geometry` swaps every leaf for the fixed fake measure so
// the frames can be diffed against the core's (`spikes/vapp-layout/frames-*`).
import { useEffect, useMemo, useRef, useState } from "react"
import {
  Avatar,
  AvatarFallback,
  Badge,
  Button,
  cn,
  conceptIcon,
  GLASS_CARD_CLASS,
  Input,
  Label,
  ListRow,
  LiveDot,
  Pill,
  Progress,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Separator,
  Switch,
  Textarea,
} from "@exp/ui"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { CONTAINER_KINDS, type VappNode } from "./fixture"
import { fixedIntrinsic } from "./measure"
import { nodeClass, resolveColor, surfaceStylesheet } from "./style-to-css"

export interface VappFrame {
  id: string
  x: number
  y: number
  w: number
  h: number
}

declare global {
  interface Window {
    __vappFrames?: () => Array<VappFrame>
  }
}

const ImageGlyph = conceptIcon(`editor-image`)
const noop = () => {}

const TEXT_VARIANT: Record<string, string> = {
  title: `text-[18px] font-semibold leading-6`,
  body: `text-sm leading-5`,
  muted: `text-[13px] leading-[18px] text-muted-foreground`,
  caption: `text-xs font-medium leading-4`,
  label: `text-[13px] font-semibold leading-[18px]`,
}

const BUTTON_VARIANT = {
  primary: `default`,
  outline: `outline`,
  ghost: `ghost`,
} as const

const str = (node: VappNode, key: string) => {
  const v = node.props?.[key]
  return typeof v === `string` ? v : ``
}

function initials(name: string) {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join(``)
}

/** Host-owned text (LANES.md "The typing test"): every edit bumps a local
 *  revision and schedules a fake host echo 150 ms later; an echo applies only
 *  when it is still the latest revision. */
function EchoField({ placeholder }: { placeholder: string }) {
  const [value, setValue] = useState(``)
  const [host, setHost] = useState(``)
  const rev = useRef(0)
  const timers = useRef<Array<number>>([])
  useEffect(() => () => timers.current.forEach((t) => window.clearTimeout(t)), [])
  return (
    <>
      <Input
        className="pr-40"
        data-testid="vapp-echo-input"
        aria-label={placeholder}
        placeholder={placeholder}
        value={value}
        onChange={(e) => {
          const next = e.target.value
          const r = ++rev.current
          setValue(next)
          timers.current.push(
            window.setTimeout(() => {
              if (r !== rev.current) return // stale echo: dropped
              setValue(next)
              setHost(next)
            }, 150)
          )
        }}
      />
      <span
        data-testid="vapp-echo-host"
        className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[11px] leading-3 text-muted-foreground"
      >
        host: {host}
      </span>
    </>
  )
}

function ToggleLeaf({ node }: { node: VappNode }) {
  const [checked, setChecked] = useState(node.props?.checked === true)
  const label = str(node, `label`)
  const inputId = `vapp-${node.id}`
  return (
    <div className="flex items-center justify-between gap-3">
      <Label htmlFor={inputId} className="text-sm font-normal">
        {label}
      </Label>
      <Switch id={inputId} aria-label={label} checked={checked} onCheckedChange={setChecked} />
    </div>
  )
}

function SelectLeaf({ node }: { node: VappNode }) {
  const options = Array.isArray(node.props?.options) ? (node.props.options as Array<string>) : []
  const [value, setValue] = useState(str(node, `value`))
  return (
    <Select value={value} onValueChange={setValue}>
      <SelectTrigger aria-label={value || `Select`} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o} value={o}>
            {o}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function Leaf({ node }: { node: VappNode }) {
  switch (node.kind) {
    case `text`: {
      const variant = str(node, `variant`) || `body`
      return <span className={TEXT_VARIANT[variant] ?? TEXT_VARIANT.body}>{str(node, `text`)}</span>
    }
    case `button`: {
      const v = (str(node, `variant`) || `primary`) as keyof typeof BUTTON_VARIANT
      return <Button variant={BUTTON_VARIANT[v] ?? `default`}>{str(node, `label`)}</Button>
    }
    case `textfield`:
      return node.props?.echo === true ? (
        <EchoField placeholder={str(node, `placeholder`)} />
      ) : (
        <Input aria-label={str(node, `placeholder`)} placeholder={str(node, `placeholder`)} />
      )
    case `textarea`:
      return (
        <Textarea
          className="min-h-0 flex-1"
          aria-label={str(node, `placeholder`)}
          placeholder={str(node, `placeholder`)}
        />
      )
    case `toggle`:
      return <ToggleLeaf node={node} />
    case `select`:
      return <SelectLeaf node={node} />
    case `listrow`:
      return (
        <ListRow className="h-8 gap-2 px-2 py-0 text-sm" aria-label={str(node, `title`)}>
          <span className="min-w-0 truncate">{str(node, `title`)}</span>
          <span className="ml-auto shrink-0 text-xs text-muted-foreground">{str(node, `meta`)}</span>
        </ListRow>
      )
    case `badge`: {
      const count = typeof node.props?.count === `number` ? node.props.count : 0
      return <Badge count={count} aria-label={`${count}`} className="h-5 min-w-5 text-[11px]" />
    }
    case `pill`: {
      const label = str(node, `label`)
      if (node.props?.tone === `live`) {
        return (
          <Pill leading={<LiveDot tone="live" ping className="size-1.5" />} className="bg-background/80">
            {label}
          </Pill>
        )
      }
      return (
        <Pill mode="select" selected={node.props?.selected === true}>
          {label}
        </Pill>
      )
    }
    case `avatar`: {
      const size = typeof node.props?.size === `number` ? node.props.size : 32
      const name = str(node, `name`)
      return (
        <Avatar aria-label={name} style={{ width: size, height: size }}>
          <AvatarFallback userId={name}>{initials(name)}</AvatarFallback>
        </Avatar>
      )
    }
    case `image`: {
      const tint = resolveColor(str(node, `placeholder`) || `$semantic.neutral`)
      return (
        <div
          role="img"
          aria-label={str(node, `alt`)}
          className="flex flex-1 items-center justify-center"
          style={{ backgroundColor: `color-mix(in srgb, ${tint} 45%, black)` }}
        >
          <ImageGlyph className="size-10 text-white/70" />
        </div>
      )
    }
    case `divider`:
      return <Separator />
    case `progress`: {
      const v = typeof node.props?.value === `number` ? node.props.value : 0
      return <Progress aria-label="Progress" value={v * 100} />
    }
    case `markdown`: {
      const text = str(node, `text`)
      return (
        <MarkdownEditor
          markdown={text}
          editable={false}
          onChange={noop}
          appearance="chat"
          ariaLabel={text.split(`\n`)[0]}
        />
      )
    }
    default:
      throw new Error(`vapp: ${node.kind} is not a leaf`)
  }
}

function GeometryLeaf({ node }: { node: VappNode }) {
  const { w, h } = fixedIntrinsic(node)
  return <div style={{ width: w, height: h, whiteSpace: `nowrap`, flexShrink: 0 }} />
}

function VappNodeView({ node, geometry }: { node: VappNode; geometry: boolean }) {
  const className = nodeClass(node.id)
  if (CONTAINER_KINDS.has(node.kind)) {
    return (
      <div
        data-vapp-id={node.id}
        className={cn(
          className,
          // The card hairline is PAINT (an inset shadow), never a border: the
          // core has no border on `card`, and a CSS border takes layout space.
          node.kind === `card` &&
            cn(GLASS_CARD_CLASS, `border-0 shadow-[inset_0_0_0_1px_var(--color-glass-stroke-card)]`)
        )}
      >
        {(node.children ?? []).map((c) => (
          <VappNodeView key={c.id} node={c} geometry={geometry} />
        ))}
      </div>
    )
  }
  return (
    <div data-vapp-id={node.id} className={className}>
      {geometry ? <GeometryLeaf node={node} /> : <Leaf node={node} />}
    </div>
  )
}

export function VappSurface({
  tree,
  geometry = false,
  width,
  rtl = false,
}: {
  tree: VappNode
  geometry?: boolean
  width?: number
  rtl?: boolean
}) {
  const css = useMemo(
    () => surfaceStylesheet(tree, { rtl, leafDisplay: geometry ? `block` : `flex` }),
    [tree, rtl, geometry]
  )
  const rootRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    window.__vappFrames = () => {
      const surface = rootRef.current?.querySelector<HTMLElement>(`[data-vapp-id="${tree.id}"]`)
      if (!surface) return []
      const origin = surface.getBoundingClientRect()
      return Array.from(surface.parentElement!.querySelectorAll<HTMLElement>(`[data-vapp-id]`)).map(
        (el) => {
          const r = el.getBoundingClientRect()
          return {
            id: el.dataset.vappId!,
            x: r.left - origin.left,
            y: r.top - origin.top,
            w: r.width,
            h: r.height,
          }
        }
      )
    }
    return () => {
      delete window.__vappFrames
    }
  }, [tree])
  return (
    <div
      ref={rootRef}
      data-testid="vapp-surface"
      // A container query cannot target its own element, so the surface root
      // sits inside this `vapp` container; `@media (min-width)` in the fixture
      // becomes `@container vapp (min-width)` = the SURFACE width.
      style={{ width: width ?? `100%`, containerType: `inline-size`, containerName: `vapp` }}
    >
      <style>{css}</style>
      <VappNodeView node={tree} geometry={geometry} />
    </div>
  )
}
