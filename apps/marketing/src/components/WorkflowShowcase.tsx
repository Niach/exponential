/* ─── WorkflowShowcase — whole boards run as one workflow (MKT-9) ───
   The server-side half of prompt (c) ("Overnight review") drawn as the
   workflow it becomes: a contract node, three parallel leaves, an
   integration node and the ONE final PR. Looks modelled on the app's
   workflow graph (packages/ui/src/workflow-graph.tsx): landed edges solid,
   speculative ones dashed; tones muted / active / success.

   Honesty note: a workflow runs every node on its ONE runner device
   (`workflows.device_id`, crates/coding/src/workflows/mod.rs), so the whole
   graph says "Home server"; prompt (c) splits devices by making two
   workflows, never by splitting one.

   Hand-drawn SSR-safe SVG with fixed viewBoxes (no DOM measuring): a
   horizontal graph for desktop, a vertical one for phones, CSS picks one.
   useScenePlayer loops the waves; beat 0 (SSR) = everything queued, reduced
   motion = the finished composite. The SVGs are aria-hidden; a text
   description sits beside them. */
import { motion } from "motion/react"
import { eyebrowDraw, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"

type NodeState = `queued` | `running` | `landed`

interface GraphNode {
  id: string
  ident: string
  title: string
  /* The beat the node's run starts, and the beat it lands. */
  runAt: number
  landAt: number
}

const NODES: GraphNode[] = [
  { id: `contract`, ident: `REV-1`, title: `Shared error type`, runAt: 1, landAt: 2 },
  { id: `retry`, ident: `REV-2`, title: `Retry sync on rate limits`, runAt: 3, landAt: 5 },
  { id: `webhook`, ident: `REV-3`, title: `Verify webhook signatures`, runAt: 3, landAt: 4 },
  { id: `pool`, ident: `REV-4`, title: `Close leaked DB connections`, runAt: 3, landAt: 6 },
  { id: `integrate`, ident: `REV-5`, title: `Errors through the API`, runAt: 7, landAt: 8 },
]

/* The final PR lights on this beat. */
const FINAL_AT = 9

const EDGES: [string, string][] = [
  [`contract`, `retry`],
  [`contract`, `webhook`],
  [`contract`, `pool`],
  [`retry`, `integrate`],
  [`webhook`, `integrate`],
  [`pool`, `integrate`],
]

/* Beat durations (ms): 0 queued · 1 contract runs · 2 lands · 3 leaves run
   · 4-6 leaves land one by one · 7 integration runs · 8 lands · 9 final PR
   ready, held before the loop restarts. */
const BEATS = [1400, 1800, 900, 2200, 700, 700, 1000, 1800, 900, 3400]
const LAST_BEAT = BEATS.length - 1

function stateAt(node: GraphNode, beat: number): NodeState {
  if (beat >= node.landAt) return `landed`
  if (beat >= node.runAt) return `running`
  return `queued`
}

const CAPTION: Record<NodeState, string> = {
  queued: `Queued`,
  running: `Running`,
  landed: `Landed`,
}

/* ── Geometry. Every box is { x, y, w, h } in viewBox units. ── */
interface Box {
  x: number
  y: number
  w: number
  h: number
}

interface Layout {
  width: number
  height: number
  boxes: Record<string, Box>
  final: Box
  edge: (from: Box, to: Box) => string
  finalEdge: (from: Box, to: Box) => string
}

/* Desktop: waves left to right (the app's own orientation). */
const NW = 214
const NH = 56
const WAVE_X = [0, 262, 524, 786]
const LANE_Y = [0, 76, 152]
const WIDE: Layout = {
  width: WAVE_X[3] + NW,
  height: LANE_Y[2] + NH,
  boxes: {
    contract: { x: WAVE_X[0], y: LANE_Y[1], w: NW, h: NH },
    retry: { x: WAVE_X[1], y: LANE_Y[0], w: NW, h: NH },
    webhook: { x: WAVE_X[1], y: LANE_Y[1], w: NW, h: NH },
    pool: { x: WAVE_X[1], y: LANE_Y[2], w: NW, h: NH },
    integrate: { x: WAVE_X[2], y: LANE_Y[1], w: NW, h: NH },
  },
  final: { x: WAVE_X[3], y: LANE_Y[1], w: NW, h: NH },
  /* The wave-graph curve: out the right edge, an S-bend in the gap. */
  edge: (a, b) => {
    const x1 = a.x + a.w
    const y1 = a.y + a.h / 2
    const x2 = b.x
    const y2 = b.y + b.h / 2
    const bend = (x2 - x1) / 2
    return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`
  },
  finalEdge: (a, b) =>
    `M ${a.x + a.w} ${a.y + a.h / 2} L ${b.x} ${b.y + b.h / 2}`,
}

/* Phones: waves top to bottom. Leaves are indented; a left spine feeds
   them from the contract, a right spine carries them into the
   integration node. */
const TW = 300
const TALL: Layout = {
  width: 340,
  height: 452,
  boxes: {
    contract: { x: 4, y: 0, w: TW, h: NH },
    retry: { x: 36, y: 86, w: TW - 12, h: NH },
    webhook: { x: 36, y: 158, w: TW - 12, h: NH },
    pool: { x: 36, y: 230, w: TW - 12, h: NH },
    integrate: { x: 4, y: 316, w: TW, h: NH },
  },
  final: { x: 4, y: 396, w: TW, h: NH },
  edge: (a, b) => {
    if (b.x > a.x) {
      /* contract → leaf: down the left spine, then into the leaf's side. */
      const sx = 20
      const y2 = b.y + b.h / 2
      return `M ${sx} ${a.y + a.h} L ${sx} ${y2 - 10} Q ${sx} ${y2}, ${sx + 10} ${y2} L ${b.x} ${y2}`
    }
    /* leaf → integration: out the leaf's right side, down the right spine. */
    const sx = a.x + a.w + 16
    const y1 = a.y + a.h / 2
    const y2 = b.y + b.h / 2
    return `M ${a.x + a.w} ${y1} L ${sx - 10} ${y1} Q ${sx} ${y1}, ${sx} ${y1 + 10} L ${sx} ${y2 - 10} Q ${sx} ${y2}, ${sx - 10} ${y2} L ${b.x + b.w} ${y2}`
  },
  finalEdge: (a, b) =>
    `M ${a.x + a.w / 2} ${a.y + a.h} L ${b.x + b.w / 2} ${b.y}`,
}

/* The state glyph in a node's top-left: hollow ring (queued), live dot with
   a CSS pulse (running), filled check (landed). */
function Glyph({ x, y, state }: { x: number; y: number; state: NodeState }) {
  return (
    <g className={`wfs-glyph`} transform={`translate(${x} ${y})`}>
      {state === `running` && <circle className={`wfs-pulse`} r={7} />}
      <circle className={`wfs-glyph-dot`} r={5} />
      {state === `landed` && (
        <path className={`wfs-glyph-check`} d={`M -2.4 0.2 L -0.6 2 L 2.6 -1.6`} />
      )}
    </g>
  )
}

function Graph({ layout, beat, variant }: { layout: Layout; beat: number; variant: string }) {
  const byId = new Map(NODES.map((n) => [n.id, n]))
  const finalLit = beat >= FINAL_AT
  const integrate = byId.get(`integrate`)!
  const integrateLanded = stateAt(integrate, beat) === `landed`
  return (
    <svg
      className={`wfs-svg wfs-svg--${variant}`}
      viewBox={`-6 -6 ${layout.width + 12} ${layout.height + 12}`}
      aria-hidden={true}
      focusable={`false`}
    >
      {/* Edges first, under the nodes: dashed until the blocker lands. */}
      {EDGES.map(([from, to]) => {
        const landed = stateAt(byId.get(from)!, beat) === `landed`
        return (
          <path
            key={`${from}-${to}`}
            className={`wfs-edge${landed ? ` is-landed` : ``}`}
            d={layout.edge(layout.boxes[from], layout.boxes[to])}
          />
        )
      })}
      <path
        className={`wfs-edge${integrateLanded ? ` is-landed` : ``}`}
        d={layout.finalEdge(layout.boxes.integrate, layout.final)}
      />

      {NODES.map((node) => {
        const b = layout.boxes[node.id]
        const state = stateAt(node, beat)
        return (
          <g key={node.id} className={`wfs-node is-${state}`}>
            <rect className={`wfs-box`} x={b.x} y={b.y} width={b.w} height={b.h} rx={8} />
            <Glyph x={b.x + 16} y={b.y + 19} state={state} />
            <text className={`wfs-ident`} x={b.x + 28} y={b.y + 23}>
              {node.ident}
            </text>
            <text className={`wfs-caption`} x={b.x + b.w - 12} y={b.y + 23} textAnchor={`end`}>
              {CAPTION[state]}
            </text>
            <text className={`wfs-title`} x={b.x + 12} y={b.y + 43}>
              {node.title}
            </text>
          </g>
        )
      })}

      {/* The one outcome: the final PR, one wave past everything. */}
      <g className={`wfs-node wfs-final${finalLit ? ` is-lit` : ``}`}>
        <rect
          className={`wfs-box`}
          x={layout.final.x}
          y={layout.final.y}
          width={layout.final.w}
          height={layout.final.h}
          rx={8}
        />
        <Glyph
          x={layout.final.x + 16}
          y={layout.final.y + 19}
          state={finalLit ? `landed` : `queued`}
        />
        <text className={`wfs-ident`} x={layout.final.x + 28} y={layout.final.y + 23}>
          Final PR
        </text>
        <text
          className={`wfs-caption`}
          x={layout.final.x + layout.final.w - 12}
          y={layout.final.y + 23}
          textAnchor={`end`}
        >
          {finalLit ? `Ready for review` : `Waiting`}
        </text>
        <text className={`wfs-title`} x={layout.final.x + 12} y={layout.final.y + 43}>
          One PR you review once
        </text>
      </g>
    </svg>
  )
}

const POINTS = [
  {
    title: `Blocks relations become the edges.`,
    text: `What has to land first runs first. Everything else runs in parallel.`,
  },
  {
    title: `Every node is its own agent run.`,
    text: `On the machine you pick for the workflow, with an agent review before each wave lands.`,
  },
  {
    title: `Nodes merge into one final PR.`,
    text: `You review the whole board once, not five branches.`,
  },
]

export function WorkflowShowcase() {
  const { ref, beat, reduced } = useScenePlayer(BEATS)
  /* Reduced motion: the finished composite, no loop. */
  const shown = reduced ? LAST_BEAT : beat

  return (
    <section className={`wfs-section`} id={`workflows`}>
      <div className={`shell`}>
        <motion.div className={`wfs-head`} {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Workflows
          </motion.span>
          <h2 className={`section-title`}>Whole boards, run as one workflow.</h2>
          <p className={`section-sub`}>
            This is what the overnight review prompt builds: the findings
            board, run as a graph of agent runs that ends in a single pull
            request.
          </p>
        </motion.div>

        <motion.div className={`wfs-stage`} {...sectionReveal}>
          <div className={`wfs-stage-bar`}>
            <span className={`wfs-stage-name`}>Review findings</span>
            <span className={`wfs-stage-meta`}>5 nodes · runs on Home server</span>
          </div>
          <div
            ref={ref}
            className={`wfs-graph${reduced ? ` is-static` : ``}`}
          >
            <Graph layout={WIDE} beat={shown} variant={`wide`} />
            <Graph layout={TALL} beat={shown} variant={`tall`} />
          </div>
          <p className={`wfs-sr`}>
            Example workflow on a review findings board. REV-1, a shared error
            type, is the contract node and runs first. REV-2, REV-3 and REV-4
            build on it and run in parallel. REV-5 integrates them. The
            workflow ends in one final pull request.
          </p>
        </motion.div>

        <ul className={`wfs-points`}>
          {POINTS.map((point) => (
            <motion.li key={point.title} className={`wfs-point`} {...sectionReveal}>
              <span className={`wfs-point-title`}>{point.title}</span>
              <span className={`wfs-point-text`}>{point.text}</span>
            </motion.li>
          ))}
        </ul>
      </div>
    </section>
  )
}
