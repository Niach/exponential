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
  icon: LucideIcon
  side: `top` | `left` | `right`
}

/* Clockwise from 12 o'clock. */
const NODES: LoopNode[] = [
  { label: `Feedback`, icon: MessageSquare, side: `top` },
  { label: `Issue`, icon: CircleDot, side: `right` },
  { label: `Code`, icon: Terminal, side: `right` },
  { label: `PR`, icon: GitPullRequest, side: `left` },
  { label: `Ship`, icon: Rocket, side: `left` },
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
      aria-label={`The loop: feedback becomes an issue, code, a pull request, and a release.`}
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
              <Icon size={17} strokeWidth={1.8} />
            </span>
            <span className={`loop-label is-${node.side}`}>{node.label}</span>
          </div>
        )
      })}
    </div>
  )
}
