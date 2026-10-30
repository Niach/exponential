import type * as React from "react"
import { useEffect, useId, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { createRoot } from "react-dom/client"
import {
  FEEDBACK_STATES,
  MARK,
  markStripePoint,
  markStripeSpan,
  type MarkSpan,
} from "@exp/domain-contract/feedback-mark"
import { Spinner } from "@exponential_app/eui-react/primitives/spinner"

import { cn } from "./cn"
import { toast } from "./toast"
import { useMediaQuery } from "./use-media-query"

// EXP-1275 (EXP-1259 D8): THE feedback states on the web, one system ×4 off
// `fixtures/feedback-states.json`: the inline Spinner (+ InlineStatus), the
// pending Button (button.tsx, gated by `useDelayedFlag`), SkeletonRows
// (skeleton.tsx), the mark (FeedbackMark) and BlockingOverlay. Errors are
// toasts (toast.tsx) or the field's own inline line; nothing here draws one.

export { Spinner }

export const FEEDBACK_PENDING_DELAY_MS: number = FEEDBACK_STATES.pending.delayMs
export const FEEDBACK_WORDS = {
  inline: FEEDBACK_STATES.inline.words,
  pending: FEEDBACK_STATES.pending.words,
  autosave: FEEDBACK_STATES.autosave.captions,
} as const
export const FEEDBACK_TEST_IDS = FEEDBACK_STATES.testIds
export const BLOCKING_COPY = FEEDBACK_STATES.blocking.copy
export const STARTING_COPY = FEEDBACK_STATES.starting

const REDUCED_MOTION = `(prefers-reduced-motion: reduce)`

/** `flag`, but only once it has held true for `ms`; false again at once.
 *  A pending state that resolves inside the delay never shows (D8.2). */
export function useDelayedFlag(flag: boolean, ms: number = FEEDBACK_PENDING_DELAY_MS): boolean {
  // A zero delay shows on the first render (a static specimen has no effects).
  const [shown, setShown] = useState(() => flag && ms <= 0)
  useEffect(() => {
    if (!flag) {
      setShown(false)
      return
    }
    if (ms <= 0) {
      setShown(true)
      return
    }
    const timer = setTimeout(() => setShown(true), ms)
    return () => clearTimeout(timer)
  }, [flag, ms])
  return flag && shown
}

/** D8.1 INLINE: the spinner + the progressive word in a band, header or
 *  caption. `saving` = spinner + "Saving", `saved` = "Saved", `idle` = the
 *  settled caption (`savedLabel`, e.g. "Edited 2m ago") or nothing. */
export function InlineStatus({
  state,
  savedLabel,
  className,
}: {
  state: `saving` | `saved` | `idle`
  savedLabel?: string
  className?: string
}) {
  const text =
    state === `saving`
      ? FEEDBACK_WORDS.autosave.saving
      : state === `saved`
        ? FEEDBACK_WORDS.autosave.saved
        : savedLabel
  if (!text) return null
  return (
    <span
      data-slot="inline-status"
      data-state={state}
      aria-live="polite"
      className={cn(`inline-flex items-center text-xs text-muted-foreground`, className)}
      style={{ gap: FEEDBACK_STATES.inline.gap }}
    >
      {state === `saving` && <Spinner size={FEEDBACK_STATES.inline.spinner} label={text} />}
      <span>{text}</span>
    </span>
  )
}

/** The SVG path of one stripe's visible span, sampled over [t0, t1] at
 *  `mark.samples` steps (the logo's 100×100 grid); `` when nothing shows. */
export function markStripePath(index: number, span: MarkSpan): string {
  if (!span) return ``
  const [t0, t1] = span
  if (t1 <= t0) return ``
  const steps = MARK.samples
  const points: string[] = []
  for (let step = 0; step <= steps; step += 1) {
    const [x, y] = markStripePoint(index, t0 + ((t1 - t0) * step) / steps)
    points.push(`${x.toFixed(2)} ${y.toFixed(2)}`)
  }
  return `M ${points.join(` L `)}`
}

const STATIC_SPANS: MarkSpan[] = MARK.stripes.map(() => [0, 1])
const spansAt = (phase: number): MarkSpan[] => MARK.stripes.map((_, index) => markStripeSpan(index, phase))

/** D8.4 THE MARK: the logo disc (foreground) whose three cut stripes sweep in
 *  turn on a `mark.loopMs` loop; the cuts show the CALLER's ground (a mask).
 *  Static logo under reduce motion. `phase` freezes one frame of the loop
 *  (the styleguide's specimens) instead of animating. */
export function FeedbackMark({
  size = 16,
  phase,
  className,
}: {
  size?: number
  phase?: number
  className?: string
}) {
  const id = useId()
  const clipId = `mark-clip-${id}`
  const maskId = `mark-mask-${id}`
  const reduced = useMediaQuery(REDUCED_MOTION)
  const paths = useRef<(SVGPathElement | null)[]>([])
  const frozen = phase !== undefined
  const initial = reduced ? STATIC_SPANS : spansAt(phase ?? 0)

  useEffect(() => {
    if (reduced || frozen || typeof requestAnimationFrame !== `function`) return
    let frame = 0
    const started = performance.now()
    const tick = (now: number) => {
      const phase = ((now - started) % MARK.loopMs) / MARK.loopMs
      spansAt(phase).forEach((span, index) => {
        paths.current[index]?.setAttribute(`d`, markStripePath(index, span))
      })
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [reduced, frozen])

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      data-slot="feedback-mark"
      data-testid={FEEDBACK_TEST_IDS.mark}
      data-static={reduced || undefined}
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={cn(`shrink-0 text-foreground`, className)}
    >
      <defs>
        <clipPath id={clipId}>
          <circle cx="50" cy="50" r="50" />
        </clipPath>
        <mask id={maskId}>
          <rect width="100" height="100" fill="white" />
          <g clipPath={`url(#${clipId})`}>
            {MARK.stripes.map((_, index) => (
              <path
                key={index}
                ref={(node) => {
                  paths.current[index] = node
                }}
                d={markStripePath(index, initial[index])}
                stroke="black"
                strokeWidth={MARK.stripeWidth}
                fill="none"
              />
            ))}
          </g>
        </mask>
      </defs>
      <circle cx="50" cy="50" r="50" fill="currentColor" mask={`url(#${maskId})`} />
    </svg>
  )
}

/** D8.5 BLOCKING: a full-window backdrop + the mark + ONE muted line. Only
 *  for switching team, switching account and signing out. */
export function BlockingOverlay({ open, label }: { open: boolean; label: string }) {
  if (!open || typeof document === `undefined`) return null
  return createPortal(<BlockingSurface label={label} />, document.body)
}

/** The overlay's surface. `contained` fills the nearest positioned ancestor
 *  instead of the window (the styleguide's specimen). */
export function BlockingSurface({ label, contained = false }: { label: string; contained?: boolean }) {
  const { blocking } = FEEDBACK_STATES
  return (
    <div
      data-slot="blocking-overlay"
      data-testid={FEEDBACK_TEST_IDS.blocking}
      role="status"
      aria-live="assertive"
      aria-busy="true"
      className={cn(
        `flex flex-col items-center justify-center`,
        contained ? `absolute inset-0` : `fixed inset-0 z-[1000]`
      )}
      style={{
        gap: blocking.gap,
        backgroundColor: `color-mix(in oklab, var(--background) ${blocking.backdropOpacity * 100}%, transparent)`,
      }}
    >
      <FeedbackMark size={blocking.markSize} />
      <p className="text-sm text-muted-foreground">{label}</p>
    </div>
  )
}

/**
 * Raise the blocking overlay from an event handler whose component may
 * unmount mid-wait (a menu item that navigates away). It mounts its own root
 * on `document.body`, shows only after `pending.delayMs` (an instant switch
 * never flashes) and returns the release; releasing twice is harmless.
 */
export function blockWindow(label: string): () => void {
  if (typeof document === `undefined`) return () => {}
  const host = document.createElement(`div`)
  host.setAttribute(`data-slot`, `blocking-host`)
  let root: ReturnType<typeof createRoot> | null = null
  const timer = setTimeout(() => {
    document.body.appendChild(host)
    root = createRoot(host)
    root.render(<BlockingSurface label={label} />)
  }, FEEDBACK_PENDING_DELAY_MS)
  let released = false
  return () => {
    if (released) return
    released = true
    clearTimeout(timer)
    root?.unmount()
    host.remove()
  }
}

/** Run `work` behind the blocking overlay (shown after the delay), released
 *  however it ends. */
export async function whileBlocking<T>(label: string, work: () => Promise<T>): Promise<T> {
  const release = blockWindow(label)
  try {
    return await work()
  } finally {
    release()
  }
}

/** Fill a blocking line's `{team}` / `{account}`. */
export function blockingLabel(kind: keyof typeof BLOCKING_COPY, name = ``): string {
  return BLOCKING_COPY[kind].replace(`{team}`, () => name).replace(`{account}`, () => name)
}

// FEED-79 (EXP-1259 D10): THE error rule, feedback-states.json `errors`. A
// failed ACTION is a toast (`toastActionError`); INVALID INPUT is the
// field's own 13px rose line (`FieldError`) next to a red field border
// (`aria-invalid`). Nothing else draws an error inline.

export const FEEDBACK_ERRORS = FEEDBACK_STATES.errors

/** Where a failure renders, by WHO started the work: a person's action =
 *  a toast, invalid input = its field's line, automatic or background work
 *  (sync, polling, a load on open or focus) = nothing new (`quiet`): it
 *  keeps its band spinner / empty state. */
export function errorSurface(kind: `action` | `input` | `background`): `toast` | `inline` | `quiet` {
  return kind === `action` ? `toast` : kind === `input` ? `inline` : `quiet`
}

/** `Couldn't <verb> <thing>`, the failed action's toast title. */
export function actionErrorTitle(verb: string, thing: string): string {
  return FEEDBACK_ERRORS.titlePattern.replace(`{verb}`, verb).replace(`{thing}`, thing)
}

/** A failed action: `toast.error(title, { description, action: Retry })`.
 *  Pass `retry` only when re-running the same call is safe. */
export function toastActionError(
  title: string,
  options: { description?: string; retry?: () => void; id?: string | number } = {}
): string | number {
  const { description, retry, id } = options
  return toast.error(title, {
    id,
    description,
    action: retry ? { label: FEEDBACK_ERRORS.retry, onClick: () => retry() } : undefined,
  })
}

/** INVALID INPUT: the line under its field, 13px in the rose foreground. Pair
 *  it with `aria-invalid` on the field (the red border). Renders nothing
 *  without a message. */
export function FieldError({
  children,
  className,
  style,
  ...props
}: Omit<React.ComponentProps<`p`>, `role`>) {
  if (children === undefined || children === null || children === false || children === ``) return null
  return (
    <p
      {...props}
      data-slot="field-error"
      role="alert"
      className={cn(`leading-5 text-destructive`, className)}
      style={{ fontSize: FEEDBACK_ERRORS.inline.size, ...style }}
    >
      {children}
    </p>
  )
}
