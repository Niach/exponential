import { useMemo, useRef, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/ui"
import { ExponentialSurface, useSurface, type HostPlugin, type IconMap } from "@exponential_app/ui-react"
import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import {
  optionRows,
  PLAN_CARD_COPY,
  PLAN_CARD_GEOMETRY,
  PLAN_CARD_STEPPER,
  PLAN_CARD_TEST_IDS,
  PLAN_CARD_TILES,
  PLAN_CARD_UPLOAD_SCHEME,
  type CardSurface,
} from "@exp/domain-contract/plan-card"
import { ASK_DOCK_COPY, PLAN_OPTIONS, type PlanOption } from "@exp/domain-contract/ask-dock"
import type { ThreadPlanQuestion, ThreadQuestionOption } from "@exp/domain-contract/session-tree"
import { ASK_OPTION_PICKED_CLASS, ASK_OPTION_ROW_CLASS } from "./ask-card"
import { Button } from "./button"
import { ChoiceTiles } from "./choice-tiles"
import { cn } from "./cn"
import { Composer } from "./composer"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "./dropdown-menu"
import { ICON_COMPONENTS, conceptIcon } from "./icons.generated"
import { Input } from "./input"
import { Textarea } from "./textarea"

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
//   PlanCardFooter        N-12: THE sheet's pinned footer: Back / Next (the
//                         pages, plan-card.json `stepper`) and on the last
//                         page THE decision: "No, keep planning" (secondary;
//                         picked = the feedback field above the footer,
//                         Cancel + the reject as the primary) and "Yes" (the
//                         primary, its menu holding "Yes, and start with a
//                         fresh context"); a question card's = Answer
//
// The pure helpers mirror the desktop's (`strip_pending_uploads`,
// `toggle_pick`, `question_answer`, `closed_picks`).
//
// Geometry = plan-card.json `card` (`PLAN_CARD_GEOMETRY`) and `tiles` (THE
// ChoiceTiles recipe). The surface frame and the tiles are PX (the fixture's
// numbers, never inflated by the desktop's 18.5px rem); the rhythm below is
// the Tailwind step equal to the number at the 16px rem, like the rest of
// the app: `blockGap` 12 = mt-3, `blockInnerGap` 2 = the caption's mt-0.5,
// `optionsGap` 6 = the options' mt-1.5 under it, `rowMinHeight` 32 = min-h-8
// / h-8, `rowRadius` 10 = rounded-md, `surfaceLabelGap` 6 = gap-1.5.

const CheckIcon = conceptIcon(`ui-check`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
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
  return planCardHost()
}

/** THE card host (the plan card's, and thread 58cb32df's `SurfaceCard`):
 *  `onAction` = the surface's action events (a surface card's `reply`). */
export function planCardHost(onAction?: (event: { name: string; context: Record<string, unknown> }) => void): HostPlugin {
  const origin = typeof window === `undefined` ? `http://localhost` : window.location.origin
  return {
    ...(onAction ? { onAction: (event) => onAction({ name: event.name, context: event.context }) } : {}),
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
          `overflow-hidden border border-glass-stroke-card bg-background/40`,
          disabled && `opacity-80`
        )}
        style={{ borderRadius: PLAN_CARD_GEOMETRY.surfaceRadius, padding: PLAN_CARD_GEOMETRY.surfacePadding }}
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
                `flex min-h-8 items-center gap-1.5 rounded-md border border-transparent px-3 py-1.5 text-xs`,
                selected ? cn(ASK_OPTION_PICKED_CLASS, `text-foreground`) : `text-muted-foreground`
              )}
              style={selected ? undefined : { opacity: PLAN_CARD_GEOMETRY.inertOpacity }}
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
  className,
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
  /** Overrides the block's top gap (a stepped question sits under its step line). */
  className?: string
}) {
  return (
    <div
      data-testid={PLAN_CARD_TEST_IDS.question}
      data-question={question.id}
      className={cn(`mt-3 flex flex-col`, className)}
    >
      <div className="text-xs font-semibold text-foreground">{question.prompt}</div>
      <div className="mt-0.5">
        <PlanCardCaption>{question.multi ? PLAN_CARD_COPY.chooseAny : PLAN_CARD_COPY.chooseOne}</PlanCardCaption>
      </div>
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

/** N-12: THE sheet's pinned footer (plan-card.json `stepper`): `onBack` /
 *  `onNext` as small secondary buttons (`buttonGap` apart, right-aligned;
 *  Back leads the row); the last page carries THE decision instead of
 *  Next: the reject option as a small secondary button, the approve option
 *  as the small primary button with a trailing menu holding the fresh
 *  option; approving is disabled while `approveIssue` names a missing
 *  answer (the issue line under the row); picking the reject button shows
 *  the feedback field above the row and the row reads Cancel + the reject
 *  as the primary. `decision` absent = a plain pager. */
export function PlanCardFooter({
  onBack,
  onNext,
  decision,
}: {
  onBack?: () => void
  onNext?: () => void
  decision?: {
    /** Why approving waits (`copy.answerIssues`), else null. */
    approveIssue: keyof typeof PLAN_CARD_COPY.answerIssues | null
    /** The reject button was picked: the feedback field shows. */
    rejecting: boolean
    onRejecting: (rejecting: boolean) => void
    feedback: string
    onFeedback: (text: string) => void
    onDecide: (option: PlanOption) => void
    /** The answer is on its way (the buttons go inert). */
    busy?: boolean
  }
}) {
  const approve = PLAN_OPTIONS.find((o) => o.key === `approve`) ?? null
  const fresh = PLAN_OPTIONS.find((o) => o.key === `approve-fresh`) ?? null
  const reject = PLAN_OPTIONS.find((o) => o.key === `reject`) ?? null
  const blocked = decision?.approveIssue ?? null
  return (
    <>
      {decision?.rejecting && (
        <Composer>
          <Textarea
            value={decision.feedback}
            onChange={(event) => decision.onFeedback(event.target.value)}
            placeholder={ASK_DOCK_COPY.feedbackPlaceholder}
            aria-label={PLAN_CARD_COPY.feedback}
            rows={1}
            autoFocus
            data-testid={PLAN_CARD_TEST_IDS.feedback}
            className="max-h-32 border-none bg-transparent px-3 pb-1 pt-3 text-sm shadow-none focus-visible:border-transparent"
            style={{ minHeight: PLAN_CARD_GEOMETRY.fieldMinHeight }}
          />
        </Composer>
      )}
      <div className="flex items-center" style={{ gap: PLAN_CARD_STEPPER.buttonGap }}>
        {onBack && (
          <Button type="button" variant="secondary" size="sm" data-testid={PLAN_CARD_TEST_IDS.stepBack} onClick={onBack}>
            {PLAN_CARD_COPY.back}
          </Button>
        )}
        <span className="ml-auto flex items-center" style={{ gap: PLAN_CARD_STEPPER.buttonGap }}>
          {onNext ? (
            <Button type="button" variant="secondary" size="sm" data-testid={PLAN_CARD_TEST_IDS.stepNext} onClick={onNext}>
              {PLAN_CARD_COPY.next}
            </Button>
          ) : decision && reject && approve ? (
            decision.rejecting ? (
              <>
                <Button type="button" variant="secondary" size="sm" onClick={() => decision.onRejecting(false)}>
                  {PLAN_CARD_COPY.cancel}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  data-slot="plan-card-decide"
                  data-decision="rejected"
                  disabled={decision.busy}
                  onClick={() => decision.onDecide(reject)}
                >
                  {reject.label}
                </Button>
              </>
            ) : (
              <>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  data-slot="plan-card-reject"
                  disabled={decision.busy}
                  onClick={() => decision.onRejecting(true)}
                >
                  {reject.label}
                </Button>
                <span className="inline-flex" data-testid={PLAN_CARD_TEST_IDS.decision}>
                  <Button
                    type="button"
                    size="sm"
                    className="rounded-r-none"
                    data-slot="plan-card-decide"
                    data-decision="approved"
                    disabled={blocked !== null || decision.busy}
                    onClick={() => decision.onDecide(approve)}
                  >
                    {approve.label}
                  </Button>
                  {fresh && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          type="button"
                          size="sm"
                          className="-ml-px rounded-l-none border-l border-primary-foreground/20 px-1.5"
                          aria-label={fresh.label}
                          disabled={blocked !== null || decision.busy}
                        >
                          <ChevronDownIcon className="size-3.5" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem data-slot="plan-card-decide-fresh" onSelect={() => decision.onDecide(fresh)}>
                          {fresh.label}
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}
                </span>
              </>
            )
          ) : null}
        </span>
      </div>
      {decision && !decision.rejecting && blocked && (
        <div data-testid={PLAN_CARD_TEST_IDS.issue} className="text-[0.6875rem] text-yellow-400">
          {PLAN_CARD_COPY.answerIssues[blocked]}
        </div>
      )}
    </>
  )
}
