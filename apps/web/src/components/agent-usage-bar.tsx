// EXP-484: the agent usage bar — how much of the running agent's rate-limit
// windows is spent, off the host machine's synced `devices.agent_usage`.
//
// Two pieces, because two surfaces need them separately:
//   `AgentUsageStrip`  — the 2 px line under a session header; click expands.
//   `AgentUsageWindows` — one row per window, also rendered always-expanded
//                         by the device-settings dialog's Agents section.
//
// Every rule (which window shows, the three tones, the countdown wording,
// the stale dimming) lives in `lib/agent-usage.ts` and is hand-mirrored on
// iOS, Android and the desktop IDE. Change a string here, change it there.
import { useState } from "react"
import type { DeviceAgentUsage, DeviceUsageWindow } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  formatResetCountdown,
  selectWindow,
  severity,
  type UsageSeverity,
} from "@/lib/agent-usage"
import {
  readAgentUsageWindow,
  writeAgentUsageWindow,
} from "@/lib/agent-usage-prefs"
import { relativeTime } from "@/components/comment-rows/format"
import { AGENT_LABELS } from "@/components/launch-dialog/launch-options-pane"

const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

const TONE: Record<UsageSeverity, string> = {
  normal: `bg-muted-foreground/60`,
  warning: `bg-amber-500`,
  danger: `bg-destructive`,
}

export function agentLabel(agent: string): string {
  return AGENT_LABELS[agent] ?? agent
}

/** `Claude Code usage: 5h 78%, resets in 2h 10m` — the strip's accessible
 * name and its hover title, so a bar with no room for text still says what
 * it means. */
function usageSummary(
  agent: string,
  window: DeviceUsageWindow,
  now: Date
): string {
  const countdown = formatResetCountdown(window.resetsAt, now)
  return `${agentLabel(agent)} usage: ${window.label} ${window.percent}%${
    countdown ? `, ${countdown}` : ``
  }`
}

/** The collapsed bar: one full-width hairline filled to the selected
 * window's percent. Renders nothing when the agent reports no windows. */
export function AgentUsageStrip({
  agent,
  usage,
  now,
}: {
  agent: string
  usage: DeviceAgentUsage
  now: Date
}) {
  const [selectedKey, setSelectedKey] = useState(() =>
    readAgentUsageWindow(agent)
  )
  const [expanded, setExpanded] = useState(false)
  const window = selectWindow(usage, selectedKey)
  if (!window) return null

  const summary = usageSummary(agent, window, now)
  return (
    <div>
      <button
        type="button"
        aria-label={summary}
        aria-expanded={expanded}
        title={summary}
        className={`flex h-3.5 w-full items-center px-0 ${
          usage.stale ? `opacity-50` : ``
        }`}
        onClick={() => setExpanded((value) => !value)}
      >
        <div className="h-0.5 w-full bg-border/60">
          <div
            className={`h-full ${TONE[severity(window.percent)]}`}
            style={{ width: `${window.percent}%` }}
          />
        </div>
      </button>
      {expanded && (
        <div className="border-b border-border/60 px-3 pb-1.5">
          <AgentUsageWindows
            agent={agent}
            usage={usage}
            now={now}
            selectedKey={selectedKey}
            onSelect={(key) => {
              setSelectedKey(key)
              writeAgentUsageWindow(agent, key)
              setExpanded(false)
            }}
          />
        </div>
      )}
    </div>
  )
}

/** Every window the machine reported, newest numbers first-hand: label, bar,
 * percent, countdown, and which one the strip is pinned to. Selecting a row
 * pins it (a local reading habit — `lib/agent-usage-prefs.ts`). */
export function AgentUsageWindows({
  agent,
  usage,
  now,
  selectedKey,
  onSelect,
}: {
  agent: string
  usage: DeviceAgentUsage
  now: Date
  selectedKey: string | null
  onSelect: (key: string) => void
}) {
  const selected = selectWindow(usage, selectedKey)
  if (usage.windows.length === 0) return null
  return (
    <div className={usage.stale ? `space-y-0.5 opacity-50` : `space-y-0.5`}>
      {usage.windows.map((window) => {
        const active = selected?.key === window.key
        const Marker = active ? SelectedIcon : UnselectedIcon
        const countdown = formatResetCountdown(window.resetsAt, now)
        return (
          <button
            key={window.key}
            type="button"
            aria-label={usageSummary(agent, window, now)}
            aria-pressed={active}
            className="flex w-full items-center gap-2 py-0.5 text-left text-[11px] text-muted-foreground hover:text-foreground"
            onClick={() => onSelect(window.key)}
          >
            <Marker className="size-3 shrink-0" />
            <span className="w-14 shrink-0 truncate">{window.label}</span>
            <span className="h-1.5 min-w-0 flex-1 rounded-full bg-border/60">
              <span
                className={`block h-full rounded-full ${TONE[severity(window.percent)]}`}
                style={{ width: `${window.percent}%` }}
              />
            </span>
            <span className="w-8 shrink-0 text-right tabular-nums">
              {window.percent}%
            </span>
            <span className="w-28 shrink-0 truncate text-right">
              {countdown ?? ``}
            </span>
          </button>
        )
      })}
      {usage.stale && (
        <p className="pl-5 text-[11px] text-muted-foreground">
          as of {relativeTime(usage.fetchedAt)}
        </p>
      )}
    </div>
  )
}
