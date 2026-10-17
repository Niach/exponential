import { useMemo, useRef, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential-at/ui"
import { ExponentialSurface, useSurface, type HostPlugin, type IconMap } from "@exponential-at/ui-react"
import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import {
  optionRows,
  PLAN_CARD_COPY,
  PLAN_CARD_TEST_IDS,
  PLAN_CARD_TILES,
  PLAN_CARD_UPLOAD_SCHEME,
  type CardSurface,
} from "@exp/domain-contract/plan-card"
import type { ThreadPlanQuestion, ThreadQuestionOption } from "@exp/domain-contract/session-tree"
import { ASK_OPTION_PICKED_CLASS, ASK_OPTION_ROW_CLASS } from "./ask-card"
import { Button } from "./button"
import { ChoiceTiles } from "./choice-tiles"
import { cn } from "./cn"
import { ICON_COMPONENTS, conceptIcon } from "./icons.generated"
import { Input } from "./input"

// EXP-1280 custom plan mode: THE rich plan / question card's form parts ×web
// (fixtures/plan-card.json; desktop `crates/ui/src/plan_card.rs`, iOS
// `PlanCardForm.swift`, Android `PlanCardView.kt`). The thread's cards
// (`session-thread-view.tsx` ThreadPlanCard / ThreadQuestionCard), THE ask
// dock and the session view's tool-posted cards paint through these:
//
//   PlanCardSurface       the agent's inline Exponential UI surface: the real
//                         renderer, a card host (no transport, `set` the only
//                         function, media = /api/attachments + https), its
//                         data model read back at answer time
//   PlanCardOptions       THE option rows (text only) or the picture tiles
//                         (`ChoiceTiles`, any option with a picture); closed =
//                         the picks highlighted, the rest inert
//   PlanCardQuestionBlock prompt (strong) + Choose one / Choose any + the
//                         options + the `custom` free-text row
//
// The pure helpers mirror the desktop's (`strip_pending_uploads`,
// `toggle_pick`, `question_answer`, `closed_picks`).

const CheckIcon = conceptIcon(`ui-check`)
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

const PENDING_UPLOAD = new RegExp(
  `!\\[[^\\]\\n]*\\]\\(${PLAN_CARD_UPLOAD_SCHEME.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)}[^)\\n]*\\)`,
  `g`
)

/** The markdown the card paints: an `![alt](upload:<label>)` embed whose
 *  upload has not landed renders NOTHING (plan-card.json). */
export function stripPendingUploads(markdown: string): string {
  return markdown.replace(PENDING_UPLOAD, ``)
}

/** A press on `label`: single = picks it (again = unpicks), multi = toggles
 *  it, kept in OPTION order. */
export function togglePick(picked: readonly string[], label: string, multi: boolean, order: readonly string[]): string[] {
  if (!multi) return picked.includes(label) ? [] : [label]
  const next = picked.includes(label) ? picked.filter((pick) => pick !== label) : [...picked, label]
  const rank = (pick: string) => {
    const at = order.indexOf(pick)
    return at < 0 ? Number.MAX_SAFE_INTEGER : at
  }
  return next.sort((a, b) => rank(a) - rank(b))
}

/** A question's answer: the picks, then the free text (a single choice
 *  takes the free text INSTEAD of a pick). */
export function questionAnswer(picked: readonly string[], customText: string, multi: boolean): string[] {
  const text = customText.trim()
  if (!text) return [...picked]
  return multi ? [...picked, text] : [text]
}

/** A closed question card's picks: `answers`, else a legacy `answer` that
 *  names an option. */
export function closedPicks(
  options: readonly { label: string }[],
  answer: string | null,
  answers: readonly string[] | null
): string[] {
  if (answers) return [...answers]
  return answer && options.some((option) => option.label === answer) ? [answer] : []
}

/** The surface's icon names: a concept id or a raw registry name (the
 *  app host's map, `exponential-ui-icons.ts`). */
const CARD_ICONS: IconMap = (name: string) => {
  const concept = SEMANTIC_ICONS[name as IconConcept] as IconName | undefined
  return ICON_COMPONENTS[(concept ?? name) as IconName]
}

/** Whether a surface media `src` may load: an own attachment (relative) or
 *  an https url (desktop `card_media_allowed`). */
export function cardMediaAllowed(src: string): boolean {
  return src.startsWith(`/api/attachments/`) || src.startsWith(`https://`)
}

function cardHost(): HostPlugin {
  const origin = typeof window === `undefined` ? `http://localhost` : window.location.origin
  return {
    icons: CARD_ICONS,
    media: { baseUrl: origin },
    urls: { baseUrl: origin },
    mediaRequest: (src) => (cardMediaAllowed(src) ? { url: new URL(src, origin).href, headers: {} } : null),
    // No transport: events reach nobody, every host function is denied
    // (`set` runs inside the renderer), links open nothing.
    onFunctionCall: () => undefined,
    openUrl: () => {},
  }
}

function documentMode(): `light` | `dark` {
  if (typeof document === `undefined`) return `dark`
  return document.documentElement.classList.contains(`dark`) ? `dark` : `light`
}

/** The agent's inline surface in its labelled frame (`copy.surface`), the
 *  card's full content width. `dataRef` holds the live data model (the
 *  answer snapshots it); `disabled` = an answered card (read-only). */
export function PlanCardSurface({
  surface,
  data,
  disabled = false,
  dataRef,
  testKey,
}: {
  surface: CardSurface
  /** The data model to show (an answered card's stored `surfaceData`). */
  data?: Record<string, unknown> | null
  disabled?: boolean
  dataRef?: { current: Record<string, unknown> | null }
  testKey: string
}) {
  const state = useSurface({
    surfaceId: `card-${testKey}`,
    catalogId: CORE_CATALOG_ID,
    initial: surface.root as NestedNode,
    data: { ...(surface.data ?? {}), ...(data ?? {}) },
  })
  if (dataRef) dataRef.current = state.data as Record<string, unknown>
  const host = useMemo(cardHost, [])
  return (
    <div
      data-testid={PLAN_CARD_TEST_IDS.surface}
      data-disabled={disabled || undefined}
      className="mt-3 flex flex-col gap-1.5"
    >
      <div className="text-[0.6875rem] font-medium text-muted-foreground">{PLAN_CARD_COPY.surface}</div>
      <div
        // `inert`: an answered card's surface takes no input at all.
        inert={disabled || undefined}
        aria-disabled={disabled || undefined}
        className={cn(
          `overflow-hidden rounded-lg border border-glass-stroke-card bg-background/40 p-3`,
          disabled && `opacity-80`
        )}
      >
        <ExponentialSurface surface={state} host={host} theme={DEFAULT_THEME_ID} mode={documentMode()} />
      </div>
    </div>
  )
}

/** One question's options: THE option rows (every option text only) or
 *  the picture tiles. `onPick` absent = closed: the picks highlighted, the
 *  rest inert. */
export function PlanCardOptions({
  options,
  multi,
  picked,
  onPick,
  attachmentSrc,
  label,
}: {
  options: readonly ThreadQuestionOption[]
  multi: boolean
  picked: readonly string[]
  onPick?: (next: string[]) => void
  attachmentSrc: (attachmentId: string) => string
  /** The question (the tiles' accessible name). */
  label: string
}) {
  const order = useMemo(() => options.map((option) => option.label), [options])
  if (options.length === 0) return null
  if (optionRows(options) === `tiles`) {
    return (
      <ChoiceTiles
        className="mt-1.5"
        data-testid={PLAN_CARD_TEST_IDS.tile}
        aria-label={label}
        columns={PLAN_CARD_TILES.columns}
        multiple={multi}
        value={multi ? picked : (picked[0] ?? null)}
        options={options.map((option) => ({
          value: option.label,
          label: option.label,
          description: option.description,
          image: option.attachmentId ? attachmentSrc(option.attachmentId) : null,
          // Closed: the picks keep their ring, the rest go inert.
          disabled: onPick ? false : !picked.includes(option.label),
        }))}
        onChange={(value) => {
          if (!onPick) return
          onPick(Array.isArray(value) ? value : value ? [value] : [])
        }}
      />
    )
  }
  return (
    <div className="mt-1.5 flex flex-col items-stretch gap-1">
      {options.map((option) => {
        const selected = picked.includes(option.label)
        if (!onPick) {
          return (
            <div
              key={option.label}
              data-testid={PLAN_CARD_TEST_IDS.option}
              data-picked={selected || undefined}
              className={cn(
                `flex min-h-7 items-center gap-1.5 rounded-md border border-transparent px-3 py-1 text-xs`,
                selected ? cn(ASK_OPTION_PICKED_CLASS, `text-foreground`) : `text-muted-foreground/70`
              )}
            >
              <span className="flex size-3.5 shrink-0 items-center justify-center">
                {selected && <CheckIcon className="size-3.5 text-emerald-500" />}
              </span>
              <span className="min-w-0 flex-1">{option.label}</span>
            </div>
          )
        }
        return (
          <Button
            key={option.label}
            type="button"
            variant="outline"
            size="sm"
            data-testid={PLAN_CARD_TEST_IDS.option}
            aria-pressed={selected}
            className={cn(ASK_OPTION_ROW_CLASS, selected && ASK_OPTION_PICKED_CLASS)}
            onClick={() => onPick(togglePick(picked, option.label, multi, order))}
          >
            {multi ? (
              selected ? (
                <SelectedIcon className="size-3.5 shrink-0 text-foreground" />
              ) : (
                <UnselectedIcon className="size-3.5 shrink-0 text-muted-foreground" />
              )
            ) : null}
            <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
              <span>{option.label}</span>
              {option.description && (
                <span className="text-[0.6875rem] font-normal text-muted-foreground">{option.description}</span>
              )}
            </span>
            {!multi && selected && <CheckIcon className="ml-auto size-3.5 shrink-0 text-foreground" />}
          </Button>
        )
      })}
    </div>
  )
}

/** The `custom` free-text row (`copy.custom`). */
export function PlanCardCustomField({
  value,
  onChange,
  label,
}: {
  value: string
  onChange: (text: string) => void
  label: string
}) {
  return (
    <Input
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={PLAN_CARD_COPY.custom}
      aria-label={`${label}: ${PLAN_CARD_COPY.custom}`}
      data-testid={PLAN_CARD_TEST_IDS.custom}
      className="mt-1 h-8 text-xs md:text-xs"
    />
  )
}

/** The choose caption under a prompt. */
export function PlanCardCaption({ children }: { children: ReactNode }) {
  return <div className="text-[0.6875rem] text-muted-foreground">{children}</div>
}

/** One plan question: the prompt (strong), Choose one / Choose any, the
 *  options, the free-text row; closed = the picks + a free answer's line. */
export function PlanCardQuestionBlock({
  question,
  picked,
  customText,
  onPick,
  onCustom,
  attachmentSrc,
  closedFree,
}: {
  question: ThreadPlanQuestion
  picked: readonly string[]
  customText: string
  /** Absent = closed. */
  onPick?: (next: string[]) => void
  onCustom?: (text: string) => void
  attachmentSrc: (attachmentId: string) => string
  /** A closed card's free answer (not an option), shown as a line. */
  closedFree?: ReactNode
}) {
  return (
    <div data-testid={PLAN_CARD_TEST_IDS.question} data-question={question.id} className="mt-3 flex flex-col gap-0.5">
      <div className="text-xs font-semibold text-foreground">{question.prompt}</div>
      <PlanCardCaption>{question.multi ? PLAN_CARD_COPY.chooseAny : PLAN_CARD_COPY.chooseOne}</PlanCardCaption>
      <PlanCardOptions
        options={question.options}
        multi={question.multi}
        picked={picked}
        onPick={onPick}
        attachmentSrc={attachmentSrc}
        label={question.prompt}
      />
      {onPick && question.custom && onCustom && (
        <PlanCardCustomField value={customText} onChange={onCustom} label={question.prompt} />
      )}
      {!onPick && closedFree}
    </div>
  )
}

/** The card's surface data ref (the answer snapshots it). */
export function useSurfaceDataRef() {
  return useRef<Record<string, unknown> | null>(null)
}
