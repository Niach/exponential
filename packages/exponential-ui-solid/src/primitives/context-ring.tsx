import { Show, splitProps } from "solid-js"
import { CONTEXT_RING_CLASSES, RING_RADIUS, RING_SIZE, RING_STROKE, RING_TONE_CLASS, ringGeometry, type RingTone } from "@exponential_app/eui/web/primitives"

import { Button, type ButtonProps } from "./button"
import { cn } from "./cn"

export { RING_RADIUS, RING_SIZE, RING_STROKE, RING_TONE_CLASS, ringGeometry } from "@exponential_app/eui/web/primitives"
export type { RingGeometry, RingTone } from "@exponential_app/eui/web/primitives"

export interface ContextRingProps extends ButtonProps {
  /** How full the context window is — null while nothing measured it. */
  percent: number | null | undefined
  /** Which of the three tones to paint. Default: `normal`. */
  tone?: RingTone
  /** Draw the empty track anyway (the run has other usage worth opening).
   *  Without it, a run with no context window renders NOTHING. */
  showEmpty?: boolean
}

export function ContextRing(props: ContextRingProps) {
  const [local, rest] = splitProps(props, [`percent`, `tone`, `showEmpty`, `class`])
  const value = () => local.percent ?? null
  const geometry = () => ringGeometry(value() ?? 0)
  return (
    <Show when={value() !== null || local.showEmpty}>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label="Usage"
        title="Usage"
        data-testid="session-context-ring"
        class={cn(CONTEXT_RING_CLASSES.root, value() === null ? RING_TONE_CLASS.normal : RING_TONE_CLASS[local.tone ?? `normal`], local.class)}
        {...rest}
      >
        <svg viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`} width={RING_SIZE} height={RING_SIZE} aria-hidden="true" class={CONTEXT_RING_CLASSES.svg}>
          <circle cx={RING_SIZE / 2} cy={RING_SIZE / 2} r={RING_RADIUS} fill="none" stroke="currentColor" stroke-width={RING_STROKE} opacity={0.2} />
          <Show when={value() !== null}>
            <circle
              cx={RING_SIZE / 2}
              cy={RING_SIZE / 2}
              r={RING_RADIUS}
              fill="none"
              stroke="currentColor"
              stroke-width={RING_STROKE}
              stroke-linecap="round"
              stroke-dasharray={String(geometry().circumference)}
              stroke-dashoffset={String(geometry().dashOffset)}
            />
          </Show>
        </svg>
      </Button>
    </Show>
  )
}
