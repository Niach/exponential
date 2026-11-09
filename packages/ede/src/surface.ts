// #EDE-8: the surface header and the Tier C document rules. Pure and
// byte-locked by fixtures/surface.json (its $comment is the spec); ede-ui
// (apps/ede/crates/ede-ui/src/doc.rs) replays the same cases in Rust.

import fixture from "../fixtures/surface.json" with { type: "json" }

export const SURFACE_VERSION: number = fixture.version
export const LAYERS = fixture.layers as readonly Layer[]
export const EDGES = fixture.edges as readonly Edge[]
export const KEYBOARD = fixture.keyboard as readonly Keyboard[]
export const PLACEMENTS = fixture.placements as readonly Placement[]
export const SURFACE_KEYS = [`layer`, `anchor`, `size`, `exclusiveZone`, `margin`, `keyboard`, `onDemand`, `popup`, `functions`] as const
export const DOCUMENT_KEYS = [`$schema`, `$comment`, `catalogId`, `surface`, `root`, `components`, `data`] as const

export type Layer = `background` | `bottom` | `top` | `overlay`
export type Edge = `top` | `bottom` | `left` | `right`
export type Keyboard = `none` | `on-demand` | `exclusive`
export type Placement = `bottom-start` | `bottom-end` | `top-start` | `top-end` | `left-start` | `right-start`

export interface Size {
  width?: number
  height?: number
}

export interface Margin {
  top: number
  right: number
  bottom: number
  left: number
}

/** A wlr-layer-shell surface, every default filled in. */
export interface LayerHeader {
  kind: `layer`
  layer: Layer
  anchor: Edge[]
  size: Size
  exclusiveZone: number
  margin: Margin
  keyboard: Keyboard
  /** Opened only when asked (a hotkey, a host function), never at start. */
  onDemand: boolean
  functions: string[]
}

/** A popup of another document's surface (an xdg_popup under one of its components). */
export interface PopupHeader {
  kind: `popup`
  popup: { parent: string; anchor: string; placement: Placement; size: { width: number; height: number } }
  functions: string[]
}

export type SurfaceHeader = LayerHeader | PopupHeader

export interface Issue {
  path: string
  message: string
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const isPx = (v: unknown): v is number => typeof v === `number` && Number.isInteger(v) && v > 0
const isPx0 = (v: unknown): v is number => typeof v === `number` && Number.isInteger(v) && v >= 0

/** A document name: lowercase letters, digits, `-` and `_`, starting with a letter or digit (ede-ui's rule). */
export const isDocumentName = (name: string): boolean => /^[a-z0-9][a-z0-9_-]*$/.test(name) && name !== `theme`

function functionsOf(input: Record<string, unknown>, issues: Issue[]): string[] {
  const functions = input.functions
  if (functions === undefined) return []
  if (!Array.isArray(functions) || !functions.every((f) => typeof f === `string`)) {
    issues.push({ path: `surface.functions`, message: `expected a list of function names` })
    return []
  }
  return functions as string[]
}

/** The header with its defaults filled in, or the issues that refuse it. */
export function normalizeSurface(input: unknown): { header: SurfaceHeader } | { issues: Issue[] } {
  if (!isRecord(input)) return { issues: [{ path: `surface`, message: `expected an object` }] }
  const issues: Issue[] = []
  for (const key of Object.keys(input)) {
    if (!(SURFACE_KEYS as readonly string[]).includes(key)) {
      issues.push({ path: `surface.${key}`, message: `unknown key; known: ${SURFACE_KEYS.join(`|`)}` })
    }
  }
  if (input.popup !== undefined) {
    for (const key of [`layer`, `anchor`, `exclusiveZone`, `margin`, `keyboard`, `onDemand`]) {
      if (input[key] !== undefined) {
        issues.push({ path: `surface.${key}`, message: `a popup surface has no layer, anchor, exclusiveZone, margin, keyboard or onDemand` })
        break
      }
    }
    const popup = input.popup
    if (!isRecord(popup)) {
      issues.push({ path: `surface.popup`, message: `expected an object {parent, anchor, placement?, size}` })
      return { issues }
    }
    const parent = typeof popup.parent === `string` && isDocumentName(popup.parent) ? popup.parent : undefined
    if (parent === undefined) issues.push({ path: `surface.popup.parent`, message: `expected a document name (lowercase letters, digits, - and _)` })
    const anchor = typeof popup.anchor === `string` && popup.anchor.length > 0 ? popup.anchor : undefined
    if (anchor === undefined) issues.push({ path: `surface.popup.anchor`, message: `expected a component id in the parent` })
    const placement = popup.placement === undefined ? `bottom-start` : (PLACEMENTS as readonly string[]).includes(popup.placement as string) ? (popup.placement as Placement) : undefined
    if (placement === undefined) issues.push({ path: `surface.popup.placement`, message: `expected one of ${PLACEMENTS.join(`|`)}` })
    let size: { width: number; height: number } | undefined
    if (popup.size === undefined) {
      issues.push({ path: `surface.popup.size`, message: `required: a popup names its size` })
    } else if (!isRecord(popup.size) || !isPx(popup.size.width) || !isPx(popup.size.height)) {
      issues.push({ path: `surface.popup.size`, message: `expected {width, height} in positive whole px` })
    } else {
      size = { width: popup.size.width, height: popup.size.height }
    }
    const functions = functionsOf(input, issues)
    if (issues.length || !parent || !anchor || !placement || !size) return { issues }
    return { header: { kind: `popup`, popup: { parent, anchor, placement, size }, functions } }
  }

  const layer = input.layer === undefined ? `top` : (LAYERS as readonly string[]).includes(input.layer as string) ? (input.layer as Layer) : undefined
  if (layer === undefined) issues.push({ path: `surface.layer`, message: `expected one of ${LAYERS.join(`|`)}` })

  let anchor: Edge[] = []
  let anchorOk = true
  if (input.anchor !== undefined) {
    if (!Array.isArray(input.anchor) || !input.anchor.every((e) => (EDGES as readonly string[]).includes(e as string))) {
      issues.push({ path: `surface.anchor`, message: `expected a list of ${EDGES.join(`|`)}` })
      anchorOk = false
    } else if (new Set(input.anchor).size !== input.anchor.length) {
      issues.push({ path: `surface.anchor`, message: `each edge at most once` })
      anchorOk = false
    } else {
      anchor = input.anchor as Edge[]
    }
  }

  const size: Size = {}
  const sizeIn = input.size === undefined ? {} : input.size
  if (!isRecord(sizeIn)) {
    issues.push({ path: `surface.size`, message: `expected {width?, height?}` })
  } else {
    const stretched = { width: anchor.includes(`left`) && anchor.includes(`right`), height: anchor.includes(`top`) && anchor.includes(`bottom`) }
    for (const axis of [`width`, `height`] as const) {
      const v = sizeIn[axis]
      if (v === undefined) {
        // A broken anchor list already refuses the header; its size rule would only repeat the noise.
        if (anchorOk && !stretched[axis]) {
          issues.push({ path: `surface.size.${axis}`, message: `required: the ${axis === `width` ? `horizontal` : `vertical`} axis is not stretched between two anchors` })
        }
      } else if (!isPx(v)) {
        issues.push({ path: `surface.size.${axis}`, message: `expected a positive whole number of px` })
      } else {
        size[axis] = v
      }
    }
  }

  let exclusiveZone = 0
  if (input.exclusiveZone !== undefined) {
    if (typeof input.exclusiveZone !== `number` || !Number.isInteger(input.exclusiveZone) || input.exclusiveZone < -1) {
      issues.push({ path: `surface.exclusiveZone`, message: `expected a whole number of px (-1 = never avoided, 0 = none)` })
    } else if (input.exclusiveZone > 0 && anchor.length === 0) {
      issues.push({ path: `surface.exclusiveZone`, message: `needs at least one anchored edge` })
    } else {
      exclusiveZone = input.exclusiveZone
    }
  }

  const margin: Margin = { top: 0, right: 0, bottom: 0, left: 0 }
  if (input.margin !== undefined) {
    if (!isRecord(input.margin)) {
      issues.push({ path: `surface.margin`, message: `expected {top?, right?, bottom?, left?}` })
    } else {
      for (const side of [`top`, `right`, `bottom`, `left`] as const) {
        const v = input.margin[side]
        if (v === undefined) continue
        if (!isPx0(v)) issues.push({ path: `surface.margin.${side}`, message: `expected a whole number of px, 0 or more` })
        else margin[side] = v
      }
    }
  }

  let keyboard: Keyboard = `none`
  if (input.keyboard !== undefined) {
    if (!(KEYBOARD as readonly string[]).includes(input.keyboard as string)) {
      issues.push({ path: `surface.keyboard`, message: `expected one of ${KEYBOARD.join(`|`)}` })
    } else if (input.keyboard === `exclusive` && layer !== undefined && layer !== `top` && layer !== `overlay`) {
      issues.push({ path: `surface.keyboard`, message: `exclusive needs the top or overlay layer` })
    } else {
      keyboard = input.keyboard as Keyboard
    }
  }

  let onDemand = false
  if (input.onDemand !== undefined) {
    if (typeof input.onDemand !== `boolean`) {
      issues.push({ path: `surface.onDemand`, message: `expected true or false` })
    } else {
      onDemand = input.onDemand
    }
  }
  const functions = functionsOf(input, issues)
  if (issues.length || layer === undefined) return { issues }
  return { header: { kind: `layer`, layer, anchor, size, exclusiveZone, margin, keyboard, onDemand, functions } }
}

interface Node {
  id?: unknown
  component?: unknown
  props?: unknown
  on?: unknown
  children?: unknown
  slots?: unknown
}

function* nodes(root: unknown): Generator<Node> {
  if (!isRecord(root)) return
  yield root as Node
  if (Array.isArray(root.children)) for (const c of root.children) yield* nodes(c)
  if (isRecord(root.slots)) for (const s of Object.values(root.slots)) yield* nodes(s)
}

/** The structural Tier C checks of a shell document; `[]` = it may go on to eui-core. */
export function documentIssues(document: unknown): Issue[] {
  if (!isRecord(document)) return [{ path: `file`, message: `expected a document object {surface, root | components, data}` }]
  const issues: Issue[] = []
  for (const key of Object.keys(document)) {
    if (!(DOCUMENT_KEYS as readonly string[]).includes(key)) issues.push({ path: key, message: `unknown key; known: ${DOCUMENT_KEYS.join(`|`)}` })
  }
  let functions: string[] = []
  if (document.surface === undefined) {
    issues.push({ path: `surface`, message: `required: how the surface sits on the screen` })
  } else {
    const normalized = normalizeSurface(document.surface)
    if (`issues` in normalized) issues.push(...normalized.issues)
    else functions = normalized.header.functions
  }
  const hasRoot = document.root !== undefined
  const hasComponents = document.components !== undefined
  let all: Node[] = []
  if (hasRoot && hasComponents) issues.push({ path: `file`, message: `expected root OR components, not both` })
  else if (!hasRoot && !hasComponents) issues.push({ path: `file`, message: `expected root (a nested tree) or components (a flat list)` })
  else if (hasRoot) all = [...nodes(document.root)]
  else if (Array.isArray(document.components)) all = document.components.filter(isRecord) as Node[]
  const seen = new Set<string>()
  for (const node of all) {
    const id = typeof node.id === `string` ? node.id : `?`
    if (seen.has(id)) issues.push({ path: `#${id}`, message: `id used more than once` })
    seen.add(id)
    const type = isRecord(node.props) ? node.props.type : (node as Record<string, unknown>).type
    if (node.component === `Input` && type === `password`) {
      issues.push({ path: `#${id}`, message: `password inputs are refused in a shell document (a sealed surface asks for passwords)` })
    }
    if (isRecord(node.on)) {
      for (const handler of Object.values(node.on)) {
        // A2UI 1.0 names the function `@call` (#VAPP-289); `call` is the 0.9 bridge.
        const call = isRecord(handler) && isRecord(handler.functionCall) ? (handler.functionCall[`@call`] ?? handler.functionCall.call) : undefined
        if (typeof call === `string` && !functions.includes(call)) {
          issues.push({ path: `#${id}`, message: `function ${call} is not in surface.functions` })
        }
      }
    }
  }
  return issues
}
