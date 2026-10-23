import { Fragment, useEffect, useRef, useState, type HTMLAttributes, type ReactNode } from "react"
import { ASK_DOCK_COPY } from "@exp/domain-contract/ask-dock"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import { Button } from "./button"
import { cn } from "./cn"
import { Composer, ComposerSubmit } from "./composer"
import { GlassCard } from "./glass-card"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"
import { Textarea } from "./textarea"

// EXP-1316 (1m): THE question / plan card ×web. The session view (the raw
// work, `agent-session.tsx`) and the thread conversation (`ThreadConversation`)
// render these same parts: the ask chrome (EXP-698/820: neutral glass, only the
// header line tinted, primary for a plan, yellow for a question), the option
// rows with their number chips (EXP-788), the inline free-text field a
// question's free-text row and a plan's reject open under themselves (EXP-820),
// the multi-select Submit, and the CLOSED state (the chosen answer, options
// gone). Answer paths stay with the host: `onAnswer(keys, labels, text?)`.

const PlanIcon = conceptIcon(`coding-plan`)
const HelpIcon = conceptIcon(`ui-help`)
const LoadingIcon = conceptIcon(`ui-loading`)
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)
const CheckIcon = conceptIcon(`ui-check`)
const CloseIcon = conceptIcon(`ui-close`)

/** The generic ask card's own words ×web (hand-mirrored ×4 with the steer
 *  views). The plan / question card's copy lives in the fixtures:
 *  `ASK_DOCK_COPY` (planReady, question, feedbackPlaceholder) and
 *  `PLAN_CARD_COPY` (answered, answering). */
export const ASK_CARD_COPY = {
  dismissed: `Dismissed`,
  submit: `Submit`,
  submitAnswers: `Submit answers`,
  /** The free-text row a thread question offers (claude's synthetic row). */
  freeTextRow: `Type something.`,
  freeTextPlaceholder: `Type your answer…`,
} as const

export interface AskOption {
  key: string
  label: string
  description?: string
  /** The free-text row: picking it opens the inline field. */
  freeText?: boolean
  /** EXP-1280: inert for now (a plan's approve while an answer is missing). */
  disabled?: boolean
}

/** THE option row's box (EXP-850 §13: a ROW, radius.md, never a capsule)
 *  and its picked state; the rich plan card's option list shares them. */
export const ASK_OPTION_ROW_CLASS = `h-auto min-h-8 w-full justify-start whitespace-normal rounded-md py-1.5 text-left text-xs`
export const ASK_OPTION_PICKED_CLASS = `border-glass-stroke-active bg-glass-active`

/** Whether picking `index` opens the inline field instead of answering: a
 *  free-text row, or a plan's reject (its LAST option). */
export function askOpensField(
  item: { planMode?: boolean; options: readonly { freeText?: boolean }[] },
  index: number
): boolean {
  const option = item.options[index]
  if (!option) return false
  if (option.freeText === true) return true
  return item.planMode === true && item.options.length >= 2 && index === item.options.length - 1
}

const HOTKEY_MAX = 9

/** The number chip of the option at `index` (`1`–`9`), else null. */
export function askOptionHotkey(index: number): string | null {
  return index >= 0 && index < HOTKEY_MAX ? String(index + 1) : null
}

/** The card chrome (N-12: a waiting card's chrome is the DockCard's; this
 *  is the transcript's closed card and the relay cards' face). */
export function AskCard({
  plan,
  label,
  meta,
  children,
  className,
  ...rest
}: {
  plan?: boolean
  label?: ReactNode
  /** A trailing note beside the label (the stepper's "k of n"). */
  meta?: ReactNode
  children: ReactNode
  className?: string
  "data-slot"?: string
  "data-answered"?: boolean
  "data-decision"?: string
  "data-testid"?: string
  "data-kind"?: string
} & Omit<HTMLAttributes<HTMLDivElement>, `children` | `className`>) {
  return (
    <GlassCard className={cn(`p-3`, className)} {...rest}>
      <div className="flex items-start gap-2">
        {plan ? (
          <PlanIcon className="mt-0.5 size-3.5 shrink-0 text-primary" />
        ) : (
          <HelpIcon className="mt-0.5 size-3.5 shrink-0 text-yellow-400" />
        )}
        <div className="min-w-0 flex-1">
          {(label !== undefined || meta !== undefined) && (
            <div className="mb-1 flex items-center gap-2">
              {label !== undefined && (
                <span
                  className={cn(
                    `truncate text-xs font-medium`,
                    plan ? `text-primary` : `text-yellow-400`
                  )}
                >
                  {label}
                </span>
              )}
              {meta}
            </div>
          )}
          {children}
        </div>
      </div>
    </GlassCard>
  )
}

/** A closed card's resolution: the chosen answer, or a dismissal. */
export function AskAnsweredLine({
  answer,
  dismissed = false,
}: {
  answer?: string | null
  dismissed?: boolean
}) {
  return (
    <div data-slot="ask-answered" className="mt-2 flex items-start gap-1.5 text-xs">
      {dismissed ? (
        <CloseIcon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      ) : (
        <CheckIcon className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
      )}
      <span className="whitespace-pre-wrap break-words font-medium text-foreground/90">
        {dismissed ? ASK_CARD_COPY.dismissed : (answer ?? PLAN_CARD_COPY.answered)}
      </span>
    </div>
  )
}

/** The inline field an option row opens under itself: the composer's card,
 *  a plain field inside. Enter sends, Shift+Enter breaks, Escape closes. */
export function AskInlineField({
  placeholder,
  allowEmpty,
  defaultValue = ``,
  onChange,
  onSubmit,
  onCancel,
}: {
  placeholder: string
  /** A plan's reject sends with nothing typed; a typed reply never does. */
  allowEmpty: boolean
  /** The draft to reopen with (the host keeps it across a lock). */
  defaultValue?: string
  /** Every keystroke, so the host can keep the draft when the field leaves. */
  onChange?: (text: string) => void
  onSubmit: (text: string) => void
  onCancel: () => void
}) {
  const [text, setText] = useState(defaultValue)
  const sendable = allowEmpty || text.trim().length > 0
  return (
    <Composer
      className="mt-0.5"
      submit={<ComposerSubmit disabled={!sendable} onClick={() => onSubmit(text)} />}
    >
      <Textarea
        autoFocus
        value={text}
        onChange={(e) => {
          setText(e.target.value)
          onChange?.(e.target.value)
        }}
        placeholder={placeholder}
        aria-label={placeholder}
        rows={1}
        data-testid="ask-inline-field"
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return
          if (e.key === `Escape`) {
            e.preventDefault()
            onCancel()
            return
          }
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            if (sendable) onSubmit(text)
          }
        }}
        className="max-h-32 min-h-9 border-none bg-transparent px-3 pb-1 pt-3 text-sm shadow-none focus-visible:border-transparent"
      />
    </Composer>
  )
}

export interface AskPromptProps {
  options: readonly AskOption[]
  multiSelect?: boolean
  planMode?: boolean
  /** `plan`/`submit` promote the first option to the primary action. */
  variant?: `default` | `plan` | `submit`
  /** Open for an answer here (else the option labels render inert). */
  answerable: boolean
  /** An answer is in flight: `Answering…` + its labels. */
  locked?: { labels: readonly string[] } | null
  /** Closed: the resolution replaces the options. */
  resolved?: { answer?: string | null; dismissed?: boolean } | null
  /** The picks a multi-select starts from (a revisited step). */
  initialPicked?: readonly string[]
  /** This is THE pending card: `1`–`9` and Enter act on it. */
  hotkeys?: boolean
  /** The answer: keys + labels; `text` = a typed reply (a question's
   *  free-text row) or a plan reject's feedback. */
  onAnswer: (keys: string[], labels: string[], text?: string) => void
  /** Notes under the options (errors, read-only hints). */
  children?: ReactNode
}

/** The interactive half of the card: the options (number chips, the
 *  multi-select picks, the inline field) or the closed resolution. */
export function AskPrompt({
  options,
  multiSelect = false,
  planMode = false,
  variant = `default`,
  answerable,
  locked = null,
  resolved = null,
  initialPicked,
  hotkeys = false,
  onAnswer,
  children,
}: AskPromptProps) {
  const [picked, setPicked] = useState<string[]>(() => [...(initialPicked ?? [])])
  /** The option whose inline field is open, by key. */
  const [typing, setTyping] = useState<string | null>(null)
  /** The inline fields' drafts by option key: the field unmounts while the
   *  card is locked, so a failed answer reopens with what was typed. */
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  // `initialPicked` re-seeds the picks when it changes (the host flips a
  // mounted card into / out of `editing`): derived during render, so the
  // reset lands in the same pass.
  const seedKey = initialPicked === undefined ? null : initialPicked.join(`\u0000`)
  const [seeded, setSeeded] = useState(seedKey)
  if (seeded !== seedKey) {
    setSeeded(seedKey)
    setPicked([...(initialPicked ?? [])])
    setTyping(null)
  }
  const open = answerable && !locked
  const item = { planMode, options }
  const listRef = useRef<HTMLDivElement | null>(null)

  const labelsFor = (keys: readonly string[]) =>
    options.filter((o) => keys.includes(o.key)).map((o) => o.label)

  const choose = (option: AskOption, index: number) => {
    if (!open || option.disabled) return
    // A row that opens the field (free text, a plan's reject) does so in a
    // multi-select too: its key never joins the picks.
    if (askOpensField(item, index)) {
      setTyping((prev) => (prev === option.key ? null : option.key))
      return
    }
    if (multiSelect) {
      setPicked((prev) =>
        prev.includes(option.key) ? prev.filter((k) => k !== option.key) : [...prev, option.key]
      )
      return
    }
    onAnswer([option.key], [option.label])
  }

  /** Escape in the field: close it and hand focus back to its option row
   *  (the field unmounts; without this focus would land on the body). */
  const cancelInline = (index: number) => {
    setTyping(null)
    const buttons = listRef.current?.querySelectorAll<HTMLButtonElement>(`[data-testid="ask-option"]`)
    buttons?.[index]?.focus()
  }

  const submitPicked = () => {
    if (!open || picked.length === 0) return
    onAnswer(picked, labelsFor(picked))
  }

  const submitInline = (option: AskOption, text: string) => {
    if (!open) return
    const trimmed = text.trim()
    if (planMode) {
      if (trimmed.length === 0) onAnswer([option.key], [option.label])
      else onAnswer([option.key], [option.label], trimmed)
      return
    }
    if (trimmed.length === 0) return
    onAnswer([option.key], [trimmed], trimmed)
  }

  // `1`-`9` pick, Enter takes the primary (or submits the picks); never while
  // a field has focus, with a modifier, or a dialog/popover in the way.
  const hot = open && hotkeys && typing === null
  const chooseRef = useRef({ choose, submitPicked })
  chooseRef.current = { choose, submitPicked }
  useEffect(() => {
    if (!hot) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return
      if (event.isComposing) return
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === `INPUT` || target.tagName === `TEXTAREA` || target.isContentEditable)
      ) {
        return
      }
      if (document.querySelector(`[role="dialog"][data-state="open"], [data-radix-popper-content-wrapper]`)) {
        return
      }
      if (event.key === `Enter`) {
        if (event.shiftKey) return
        if (multiSelect) {
          chooseRef.current.submitPicked()
          event.preventDefault()
          return
        }
        const primary = options[0]
        if (!primary) return
        event.preventDefault()
        chooseRef.current.choose(primary, 0)
        return
      }
      if (!/^[1-9]$/.test(event.key)) return
      const index = Number(event.key) - 1
      const option = options[index]
      if (!option) return
      event.preventDefault()
      chooseRef.current.choose(option, index)
    }
    window.addEventListener(`keydown`, onKeyDown)
    return () => window.removeEventListener(`keydown`, onKeyDown)
  }, [hot, options, multiSelect])

  if (locked) {
    return (
      <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
        <LoadingIcon className="size-3 shrink-0 animate-spin" />
        <span className="shrink-0">{PLAN_CARD_COPY.answering}</span>
        {locked.labels.length > 0 && (
          <span className="truncate font-medium text-foreground/80">{locked.labels.join(`, `)}</span>
        )}
      </div>
    )
  }
  if (resolved) {
    return <AskAnsweredLine answer={resolved.answer} dismissed={resolved.dismissed === true} />
  }

  return (
    <>
      <div ref={listRef} className="mt-2 flex flex-col items-stretch gap-1">
        {options.map((option, index) => {
          const primary = variant !== `default` && index === 0
          const selected = picked.includes(option.key)
          const fieldOpen = typing === option.key
          const chip = askOptionHotkey(index)
          const label =
            variant === `submit` && index === 0 ? ASK_CARD_COPY.submitAnswers : option.label
          if (!open) {
            return (
              <span
                key={option.key}
                data-testid="ask-option"
                className="flex items-baseline gap-1.5 text-xs text-muted-foreground"
              >
                {chip && <span className="font-mono">{chip}</span>}
                <span>{label}</span>
              </span>
            )
          }
          return (
            <Fragment key={option.key}>
              <Button
                variant={primary ? `default` : `outline`}
                size="sm"
                data-testid="ask-option"
                disabled={option.disabled}
                className={cn(
                  ASK_OPTION_ROW_CLASS,
                  !primary && (selected || fieldOpen) && ASK_OPTION_PICKED_CLASS
                )}
                aria-pressed={multiSelect ? selected : undefined}
                aria-expanded={askOpensField(item, index) ? fieldOpen : undefined}
                onClick={() => choose(option, index)}
              >
                {multiSelect ? (
                  selected ? (
                    <SelectedIcon className="size-3.5 shrink-0 text-foreground" />
                  ) : (
                    <UnselectedIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  )
                ) : null}
                <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
                  <span>{label}</span>
                  {option.description && (
                    <span
                      className={cn(
                        `font-normal text-[0.6875rem]`,
                        primary ? `text-primary-foreground/80` : `text-muted-foreground`
                      )}
                    >
                      {option.description}
                    </span>
                  )}
                </span>
                {chip && (
                  <kbd
                    className={cn(
                      `ml-auto shrink-0 rounded-sm border px-1 font-mono text-[0.625rem] font-normal leading-4`,
                      primary
                        ? `border-primary-foreground/30 text-primary-foreground/80`
                        : `border-glass-stroke-card text-muted-foreground`
                    )}
                  >
                    {chip}
                  </kbd>
                )}
              </Button>
              {fieldOpen && (
                <AskInlineField
                  placeholder={
                    planMode
                      ? ASK_DOCK_COPY.feedbackPlaceholder
                      : ASK_CARD_COPY.freeTextPlaceholder
                  }
                  allowEmpty={planMode}
                  defaultValue={drafts[option.key] ?? ``}
                  onChange={(text) => setDrafts((prev) => ({ ...prev, [option.key]: text }))}
                  onSubmit={(text) => submitInline(option, text)}
                  onCancel={() => cancelInline(index)}
                />
              )}
            </Fragment>
          )
        })}
      </div>
      {open && multiSelect && (
        <Pill
          size="sm"
          mode="action"
          className="mt-2"
          disabled={picked.length === 0}
          onClick={submitPicked}
        >
          {ASK_CARD_COPY.submit}
        </Pill>
      )}
      {children}
    </>
  )
}
