import type { LucideIcon } from "lucide-react"
import {
  CircleDot,
  GitPullRequest,
  MessageSquare,
  Rocket,
  Terminal,
} from "lucide-react"

/* Ring radius as a % of the square figure (SVG viewBox is 100×100). */
const RADIUS = 37.6

type LoopNode = {
  label: string
  phrase: string
  icon: LucideIcon
  side: `top` | `left` | `right`
}

/* Clockwise from 12 o'clock. One short phrase per stage — no paragraphs. */
const NODES: LoopNode[] = [
  { label: `Feedback`, phrase: `a user reports a bug`, icon: MessageSquare, side: `top` },
  { label: `Issue`, phrase: `lands on the board`, icon: CircleDot, side: `right` },
  { label: `Code`, phrase: `Claude writes the fix`, icon: Terminal, side: `right` },
  { label: `PR`, phrase: `review, merge`, icon: GitPullRequest, side: `left` },
  { label: `Ship`, phrase: `the reporter hears back`, icon: Rocket, side: `left` },
]

const nodePos = (index: number) => {
  const angle = ((-90 + index * 72) * Math.PI) / 180
  return {
    left: `${50 + RADIUS * Math.cos(angle)}%`,
    top: `${50 + RADIUS * Math.sin(angle)}%`,
  }
}

export function LoopCircle() {
  return (
    <div
      className={`loop-figure`}
      role={`img`}
      aria-label={`The loop: a user reports a bug, it lands on the board as an issue, Claude writes the fix, the pull request merges, and the reporter hears back.`}
    >
      <svg className={`loop-ring`} viewBox={`0 0 100 100`} aria-hidden>
        <circle cx={50} cy={50} r={RADIUS} />
      </svg>
      <div className={`loop-orbit`} aria-hidden>
        <span className={`loop-dot`} />
      </div>
      {NODES.map((node, i) => {
        const Icon = node.icon
        return (
          <div key={node.label} className={`loop-node`} style={nodePos(i)}>
            <span className={`loop-chip`}>
              <Icon size={16} strokeWidth={1.8} />
            </span>
            <span className={`loop-label is-${node.side}`}>
              <span className={`loop-label-name`}>{node.label}</span>
              <span className={`loop-label-phrase`}>{node.phrase}</span>
            </span>
          </div>
        )
      })}
    </div>
  )
}
