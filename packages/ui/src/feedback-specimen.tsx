import type { ReactNode } from "react"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"

import { Button } from "./button"
import { BlockingSurface, FeedbackMark, InlineStatus, blockingLabel } from "./feedback"
import { GlassSectionHeader } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { Input } from "./input"
import { SkeletonRows } from "./skeleton"
import { ToastSpecimen } from "./toast"

// EXP-1275 + EXP-1271 + FEED-79 (EXP-1259 D8-D10): THE feedback states, the
// six kinds of `feedback-states.json` side by side (draft 6). Every specimen
// is the REAL part at rest: the pending Button with a zero delay (the live
// one waits `pending.delayMs`), the mark frozen at four phases of its loop
// (`phase`), the blocking surface contained in its card instead of the
// window, the error toast as `ToastSpecimen`. Drawn by the styleguide entry
// `feedback-states` and, for the `feedback-states` capture, by the web app's
// dev-only `/t/$teamSlug/agent?dev=feedback-states` (the kinds never show
// together in the app).

const { pending, starting, errors, mark } = FEEDBACK_STATES
const UiSubmitIcon = conceptIcon(`ui-submit`)
const MARK_PHASES = [0.08, 0.2, 0.32, 0.5]

function Kind({ index, title, children }: { index: number; title: string; children: ReactNode }) {
  return (
    <div className="grid content-start gap-3 rounded-xl border border-border p-4">
      <p className="text-sm font-medium">{`${index} · ${title}`}</p>
      {children}
    </div>
  )
}

function Band({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex h-10 items-center justify-between rounded-md bg-muted/40 px-3 text-sm">
      <span>{label}</span>
      {children}
    </div>
  )
}

export function FeedbackStatesSpecimen(): ReactNode {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(20rem,1fr))] gap-4" data-testid="feedback-states-specimen">
      <Kind index={1} title="Inline">
        <Band label="Team prompt">
          <InlineStatus state="saving" />
        </Band>
        <Band label="Team prompt">
          <InlineStatus state="saved" />
        </Band>
        <GlassSectionHeader label="MCP servers" count={3} loading />
      </Kind>
      <Kind index={2} title="Pending action">
        <div className="flex flex-wrap items-center gap-2">
          <Button>
            <UiSubmitIcon />
            Start
          </Button>
          <Button pending pendingDelayMs={0} pendingLabel={pending.words.start}>
            <UiSubmitIcon />
            Start
          </Button>
          <Button variant="outline" pending pendingDelayMs={0} pendingLabel={pending.words.connect}>
            Connect
          </Button>
        </div>
      </Kind>
      <Kind index={3} title="Placeholder">
        <SkeletonRows />
      </Kind>
      <Kind index={4} title="The mark">
        <div className="flex items-center gap-4">
          {MARK_PHASES.map((phase) => (
            <FeedbackMark key={phase} size={mark.sizes.styleguide} phase={phase} />
          ))}
        </div>
        <div className="flex items-center gap-2 rounded-xl border border-border px-3 py-2.5 text-sm">
          <FeedbackMark size={mark.sizes.inline} phase={0.3} />
          <span className="flex-1">{starting.label.replace(`{device}`, `macbook`)}</span>
          <span className="text-xs tabular-nums text-muted-foreground">1.4 s</span>
        </div>
      </Kind>
      <Kind index={5} title="Blocking">
        <div className="relative h-40 overflow-hidden rounded-lg border border-border">
          <BlockingSurface contained label={blockingLabel(`switchTeam`, `Personal`)} />
        </div>
      </Kind>
      <Kind index={6} title="Errors">
        <ToastSpecimen kind="error" title={errors.cases[0]!.title!} description="Network error" action={errors.retry} />
        <div className="grid gap-1.5">
          <Input aria-invalid defaultValue="dennis@" />
          <p className="text-[13px] text-destructive">{errors.cases[4]!.message}</p>
        </div>
      </Kind>
    </div>
  )
}
