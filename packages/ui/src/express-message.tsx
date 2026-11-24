// #EXP-2067 (fixtures/chat-express.json): an agent text with Express blocks
// in the web transcript. The row splits into its segments in order: each
// prose run THE MessageBubble (the caller's `renderProse`), each block ONE
// item drawn by THE plan card surface painter in THE surface card frame
// (one frame per surface of the block, `layout.blockGap` apart), THE inline
// marker under a failed / tooMany / tooLarge block, THE surface card
// skeleton while an open block has no surface yet (and while the Express
// wasm chunk loads: only a row that passes the gate loads it).
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID } from "@exponential_app/eui"
import { ExponentialSurface, useSurface, type A2uiMessage } from "@exponential_app/eui-react"
import {
  EXPRESS_COPY,
  ExpressRow,
  hasExpressBlock,
  plainProse,
  withoutHeldTail,
  type ActionEvent,
  type ActionOrigin,
  type BlockState,
  type ExpressSdk,
  type RowSegment,
} from "@exp/domain-contract/chat-express"
import fixture from "@exp/domain-contract/fixtures/chat-express.json"
import { useColorMode } from "./color-mode"
import { planCardHost, useCardLinks } from "./plan-card"
import { SkeletonRows } from "./skeleton"
import { SurfaceCardFrame, TranscriptMarker } from "./surface-card"

const BLOCK_GAP = fixture.layout.blockGap

// ── The SDK (the lazy wasm chunk) ──────────────────────────────────────────

let sdk: ExpressSdk | null = null
let loading: Promise<ExpressSdk | null> | null = null
const waiting = new Set<() => void>()

/** The eui SDK's Express compiler, loaded once on first need. */
function loadExpressSdk(): Promise<ExpressSdk | null> {
  loading ??= import(`@exponential_app/eui/express`)
    .then(async (m) => {
      await m.loadExpress()
      sdk = { unwrap: (text) => m.unwrapExpress(text), decoder: (surfaceId) => new m.ExpressDecoder({ surfaceId, wrapped: false }) }
      return sdk
    })
    .catch(() => null)
    .finally(() => {
      for (const done of waiting) done()
      waiting.clear()
    })
  return loading
}

/** Specimens and tests: hand the SDK over (the committed wasm, loaded). */
export function provideExpressSdk(given: ExpressSdk): void {
  sdk = given
}

function useExpressSdk(needed: boolean): { sdk: ExpressSdk | null; failed: boolean } {
  const [, bump] = useState(0)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    if (!needed || sdk) return
    let alive = true
    void loadExpressSdk().then((loaded) => {
      if (!alive) return
      if (loaded) bump((n) => n + 1)
      else setFailed(true)
    })
    return () => {
      alive = false
    }
  }, [needed])
  return { sdk: needed ? sdk : null, failed }
}

// ── The row ────────────────────────────────────────────────────────────────

export interface ExpressProsePart {
  /** The row's first segment (name, avatar, tail). */
  first: boolean
  /** The row's last prose run (the `HH:mm`). */
  last: boolean
  key?: string
}

export function ExpressMessage({
  rowKey,
  text,
  final,
  messageId,
  live = false,
  renderProse,
  fallback,
  onAction,
}: {
  /** `syncedRowKey` / `feedRowKey`: the blocks' keys and surface ids. */
  rowKey: string
  text: string
  /** A synced row, or a feed row followed by its lane's next / turn ended. */
  final: boolean
  /** The synced row's `messages.id`, null for a live feed row. */
  messageId: string | null
  /** The row arrived while the chat was open: its surfaces enter. */
  live?: boolean
  renderProse: (text: string, part: ExpressProsePart) => ReactNode
  /** The row without a block (the gate fails): today's bubble. */
  fallback: ReactNode
  onAction?: (event: ActionEvent, origin: ActionOrigin) => void
}) {
  const shown = withoutHeldTail(text, final)
  const gated = hasExpressBlock(shown)
  const { sdk: loaded, failed } = useExpressSdk(gated)
  // A row first seen streaming or arriving live: its surfaces enter once.
  const entering = useRef(live || !final)
  if (live) entering.current = true
  const rowRef = useRef<{ key: string; row: ExpressRow } | null>(null)
  const segments = useMemo<RowSegment[] | null>(() => {
    if (!gated || !loaded) return null
    if (rowRef.current?.key !== rowKey) rowRef.current = { key: rowKey, row: new ExpressRow(rowKey, loaded) }
    return rowRef.current.row.update(text, final)
  }, [gated, loaded, rowKey, text, final])
  const actionRef = useRef(onAction)
  actionRef.current = onAction

  if (!gated) {
    // A held tail (`<a2u` still arriving): the prose before it.
    if (shown !== text) return shown.trim() ? <>{renderProse(shown.trim(), { first: true, last: true })}</> : null
    return <>{fallback}</>
  }
  if (!segments) {
    // The compiler is still loading (or failed): the pure prose, then THE
    // skeleton (THE marker when the chunk never came).
    const prose = plainProse(shown)
    return (
      <div className="flex min-w-0 flex-col" style={{ gap: BLOCK_GAP }} data-slot="express-row">
        {prose ? renderProse(prose, { first: true, last: true }) : null}
        {failed ? <TranscriptMarker label={EXPRESS_COPY.blockFailed} /> : <BlockSkeleton />}
      </div>
    )
  }
  const lastProse = segments.reduce((at, s, i) => (s.kind === `prose` ? i : at), -1)
  return (
    <div className="flex min-w-0 flex-col" style={{ gap: BLOCK_GAP }} data-slot="express-row">
      {segments.map((segment, index) =>
        segment.kind === `prose` ? (
          renderProse(segment.text, { first: index === 0, last: index === lastProse, key: `${rowKey}:p${index}` })
        ) : (
          <ExpressBlock
            key={segment.key}
            block={segment}
            messageId={messageId}
            enter={entering.current}
            onAction={onAction ? (event, origin) => actionRef.current?.(event, origin) : undefined}
          />
        )
      )}
    </div>
  )
}

function BlockSkeleton() {
  return (
    <SurfaceCardFrame>
      <SkeletonRows rows={2} rowHeight={24} dot={false} />
    </SurfaceCardFrame>
  )
}

const BROKEN = new Set<BlockState[`status`]>([`failed`, `tooMany`, `tooLarge`])

function ExpressBlock({
  block,
  messageId,
  enter,
  onAction,
}: {
  block: BlockState
  messageId: string | null
  enter: boolean
  onAction?: (event: ActionEvent, origin: ActionOrigin) => void
}) {
  return (
    <div className="flex min-w-0 flex-col" style={{ gap: BLOCK_GAP }} data-testid="express-block" data-status={block.status}>
      {block.surfaces.map((surfaceId) => (
        <SurfaceCardFrame key={surfaceId} animate={enter} stagger={false}>
          <BlockSurface
            surfaceId={surfaceId}
            messages={block.messages}
            generation={block.generation}
            onAction={onAction ? (event) => onAction(event, { surfaceId, block: block.key, messageId }) : undefined}
          />
        </SurfaceCardFrame>
      ))}
      {block.status === `open` && block.surfaces.length === 0 ? <BlockSkeleton /> : null}
      {BROKEN.has(block.status) ? <TranscriptMarker label={EXPRESS_COPY.blockFailed} /> : null}
    </div>
  )
}

const surfaceOf = (message: unknown): string | undefined => {
  const m = message as A2uiMessage
  return (m.createSurface ?? m.updateComponents ?? m.updateDataModel ?? m.deleteSurface)?.surfaceId
}

/** One surface of a block: its stream's messages for it, applied as they
 *  come (a restart = a new `generation`: the store starts over, the frame
 *  stays). The block's local state is the reader's alone. */
function BlockSurface({
  surfaceId,
  messages,
  generation,
  onAction,
}: {
  surfaceId: string
  messages: readonly unknown[]
  generation: number
  onAction?: (event: ActionEvent) => void
}) {
  const state = useSurface({ surfaceId, catalogId: CORE_CATALOG_ID })
  const applied = useRef({ generation, count: 0 })
  const { apply, reset } = state
  useLayoutEffect(() => {
    if (applied.current.generation !== generation) {
      reset()
      applied.current = { generation, count: 0 }
    }
    for (let i = applied.current.count; i < messages.length; i++) {
      if (surfaceOf(messages[i]) === surfaceId) apply(messages[i] as A2uiMessage)
    }
    applied.current.count = messages.length
  })
  const actionRef = useRef(onAction)
  actionRef.current = onAction
  const links = useCardLinks()
  const host = useMemo(() => planCardHost((event) => actionRef.current?.(event), links), [links])
  if (!state.root) return <SkeletonRows rows={1} rowHeight={24} dot={false} />
  const mode = useColorMode()
  return <ExponentialSurface surface={state} host={host} theme={DEFAULT_THEME_ID} mode={mode} />
}
