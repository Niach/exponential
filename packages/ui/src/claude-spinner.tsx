import { ClaudeIcon, CLAUDE_FILL } from "./brand-icons"
import { CLAUDE_SPINNER_FRAMES } from "./claude-spinner.generated"
import { cn } from "./cn"

// EXP-1184: Claude's working mark — the hand-drawn "writing" spark claude.ai
// shows while a session works, vendored once as a sprite sheet
// (`packages/icons/agent/claude-writing.svg`) and generated into every
// client. The frames stack in one tall SVG that a CSS `steps(8)` animation
// slides through (the claude.ai technique: hard cuts, no tweening); reduced
// motion draws the static mark instead.

export function ClaudeSpinner({ className }: { className?: string }) {
  const count = CLAUDE_SPINNER_FRAMES.length
  return (
    <span
      data-slot="claude-spinner"
      role="img"
      aria-label="Working"
      className={cn(`relative inline-block size-3 shrink-0 overflow-hidden`, className)}
    >
      <svg
        viewBox={`0 0 100 ${count * 100}`}
        fill={CLAUDE_FILL}
        aria-hidden="true"
        className="absolute inset-x-0 top-0 block w-full motion-safe:animate-claude-writing motion-reduce:hidden"
        style={{ height: `${count * 100}%` }}
      >
        {CLAUDE_SPINNER_FRAMES.map((d, i) => (
          <path key={i} d={d} transform={`translate(0 ${i * 100})`} />
        ))}
      </svg>
      <ClaudeIcon className="hidden size-full motion-reduce:block" />
    </span>
  )
}
