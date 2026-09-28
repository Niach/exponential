import { useRef, useState, type ReactNode } from "react"
import { Link, useParams } from "@tanstack/react-router"
import { MonitorUp } from "lucide-react"
import {
  Button,
  Pill,
  Popover,
  PopoverAnchor,
  PopoverContent,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  conceptIcon,
  useIsMobile,
} from "@exp/ui"
import {
  READINESS_COPY,
  type CodingReadiness,
  type ReadinessFix,
  type ReadinessStep,
  type ReadinessStepKey,
} from "@/lib/coding-readiness"
import { desktopDownloadHref } from "@/lib/desktop-download"
import { cn } from "@/lib/utils"
import {
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
} from "@/components/github-repo-picker"
import { ReadinessRepoPicker } from "@/components/coding-readiness-repo-picker"
import { AddDeviceDialog } from "@/components/add-device-dialog"
import type { CodingReadinessState } from "@/hooks/use-coding-readiness"
import { toast } from "sonner"

// EXP-1121: the "Ready to code?" checklist. Start coding ALWAYS renders for a
// member; while a step is missing it is a dashed amber capsule, and a click
// opens this checklist — anchored under the capsule on a pointer device, a
// bottom sheet on a phone — with the fix for the current step inline. The
// model + every word come from `lib/coding-readiness.ts` (fixture-locked ×4);
// this file only draws it and wires each fix to the surface that already
// exists for it.

const CheckIcon = conceptIcon(`ui-check`)
const CloseIcon = conceptIcon(`ui-close`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const PlayIcon = conceptIcon(`action-run`)

const STEP_ICONS: Record<ReadinessStepKey, ReturnType<typeof conceptIcon>> = {
  github: conceptIcon(`ui-github`),
  repository: conceptIcon(`ui-branch`),
  device: conceptIcon(`ui-device`),
}

const FIX_ICONS: Partial<Record<ReadinessFix, ReturnType<typeof conceptIcon>>> =
  {
    connect_github: conceptIcon(`ui-github`),
    choose_repository: conceptIcon(`ui-branch`),
    open_devices: conceptIcon(`nav-devices`),
    get_desktop_app: conceptIcon(`ui-download`),
    set_up_server: conceptIcon(`ui-server`),
  }

const FIX_LABELS: Record<ReadinessFix, string> = {
  connect_github: READINESS_COPY.fixConnectGithub,
  choose_repository: READINESS_COPY.fixChooseRepository,
  board_settings: READINESS_COPY.fixBoardSettings,
  open_devices: READINESS_COPY.fixOpenDevices,
  get_desktop_app: READINESS_COPY.fixGetDesktopApp,
  set_up_server: READINESS_COPY.fixSetUpServer,
}

/** The amber the whole not-ready language shares (dot, capsule, ring). */
export const READINESS_AMBER = `var(--color-amber-400)`

/** Dot + "Needs a repository" — the capsule's one-line caption. */
export function ReadinessCaption({
  caption,
  onClick,
  className,
}: {
  caption: string
  onClick?: () => void
  className?: string
}) {
  return (
    <Button
      variant="text"
      size="inline"
      className={cn(`min-w-0 gap-1.5 text-amber-400 hover:text-amber-300`, className)}
      onClick={onClick}
      data-testid="coding-readiness-caption"
    >
      <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-amber-400" />
      <span className="truncate">{caption}</span>
    </Button>
  )
}

/** The Start coding capsule itself: today's pill once ready, the dashed
 * amber one while a step is missing, inert (no caption, no dashes) while the
 * inputs load. */
export function ReadinessStartPill({
  readiness,
  tone,
  onClick,
  label = READINESS_COPY.start,
  hideLabel = false,
  icon,
  className,
  testId,
}: {
  readiness: CodingReadiness
  /** Ready look: the accent fill, or glass beside a primary Merge. */
  tone: `primary` | `glass`
  onClick: () => void
  label?: string
  /** Icon-only (a narrow bar); the label stays the accessible name. */
  hideLabel?: boolean
  icon?: ReactNode
  className?: string
  testId?: string
}) {
  const notReady = !readiness.loading && !readiness.ready
  return (
    <Pill
      size="md"
      mode="action"
      primary={!notReady && tone === `primary`}
      aria-label={label}
      aria-disabled={readiness.loading || undefined}
      data-readiness={
        readiness.loading ? `loading` : readiness.ready ? `ready` : `missing`
      }
      data-testid={testId}
      className={cn(
        readiness.loading && `pointer-events-none`,
        notReady &&
          `border-dashed border-amber-400/60 bg-transparent text-foreground/70 [&_svg]:text-muted-foreground`,
        className
      )}
      onClick={readiness.loading ? undefined : onClick}
    >
      {icon ?? <MonitorUp />}
      {!hideLabel && label}
    </Pill>
  )
}

function StepGlyph({ step }: { step: ReadinessStep }) {
  if (step.state === `met`) {
    return (
      <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-400">
        <CheckIcon className="size-4" />
      </span>
    )
  }
  const Glyph = STEP_ICONS[step.key]
  return (
    <span
      className={cn(
        `flex size-7 shrink-0 items-center justify-center rounded-full border-[1.5px]`,
        step.state === `current`
          ? `border-amber-400 text-amber-400`
          : `border-dashed border-muted-foreground/50 text-muted-foreground`
      )}
    >
      <Glyph className="size-3.5" />
    </span>
  )
}

function FixButtons({
  step,
  state,
  onChooseRepository,
  onSetUpServer,
  onNavigate,
}: {
  step: ReadinessStep
  state: CodingReadinessState
  onChooseRepository: () => void
  onSetUpServer: () => void
  /** A fix that leaves the surface closes it first. */
  onNavigate: () => void
}) {
  const { teamSlug } = useParams({ strict: false })
  const board = state.board
  // Owner-only destinations stay hidden from everyone else (the board
  // settings page is owner-gated).
  const fixes = step.fixes.filter(
    (fix) =>
      fix !== `board_settings` || (state.isOwner && board)
  )
  if (fixes.length === 0) return null
  const userAgent = typeof navigator === `undefined` ? `` : navigator.userAgent
  const touchPoints =
    typeof navigator === `undefined` ? 0 : navigator.maxTouchPoints

  return (
    <div className="mt-2 flex flex-wrap items-center gap-2">
      {fixes.map((fix, index) => {
        const Glyph = FIX_ICONS[fix]
        const body = (
          <>
            {Glyph && <Glyph />}
            {FIX_LABELS[fix]}
            {fix === `board_settings` && <ChevronIcon />}
          </>
        )
        const common = {
          size: `sm` as const,
          mode: `action` as const,
          // The first fix is the one to press.
          primary: index === 0,
          "data-testid": `readiness-fix-${fix}`,
        }
        if (fix === `choose_repository`) {
          return (
            <Pill key={fix} {...common} onClick={onChooseRepository}>
              {body}
            </Pill>
          )
        }
        if (fix === `set_up_server`) {
          return (
            <Pill key={fix} {...common} onClick={onSetUpServer}>
              {body}
            </Pill>
          )
        }
        if (fix === `get_desktop_app`) {
          return (
            <Pill key={fix} {...common} asChild>
              <a
                href={desktopDownloadHref(userAgent, touchPoints)}
                target="_blank"
                rel="noreferrer"
              >
                {body}
              </a>
            </Pill>
          )
        }
        if (fix === `connect_github`) {
          // The GitHub hop opens as a popup over the issue — the person
          // never leaves it, and the rows re-probe once focus comes back.
          // Without the App's connect URL, Settings › Repositories.
          const url = state.githubConnectUrl
          if (url || !teamSlug) {
            return (
              <Pill
                key={fix}
                {...common}
                onClick={() => {
                  if (!openGithubPopup(url)) toast.error(POPUP_BLOCKED_MESSAGE)
                }}
              >
                {body}
              </Pill>
            )
          }
          return (
            <Pill key={fix} {...common} asChild>
              <Link
                to="/t/$teamSlug/settings/repositories"
                params={{ teamSlug }}
                onClick={onNavigate}
              >
                {body}
              </Link>
            </Pill>
          )
        }
        if (!teamSlug) return null
        if (fix === `board_settings` && board) {
          return (
            <Pill key={fix} {...common} asChild>
              <Link
                to="/t/$teamSlug/settings/boards/$boardId"
                params={{ teamSlug, boardId: board.id }}
                onClick={onNavigate}
              >
                {body}
              </Link>
            </Pill>
          )
        }
        if (fix === `open_devices`) {
          return (
            <Pill key={fix} {...common} asChild>
              <Link
                to="/t/$teamSlug/devices"
                params={{ teamSlug }}
                onClick={onNavigate}
              >
                {body}
              </Link>
            </Pill>
          )
        }
        return null
      })}
    </div>
  )
}

/** The three rows — met / current (fixes inline) / pending. Shared by the
 * checklist and the Getting started coding entry. */
export function ReadinessSteps({
  state,
  onSetUpServer,
  onNavigate,
  className,
}: {
  state: CodingReadinessState
  onSetUpServer?: () => void
  onNavigate?: () => void
  className?: string
}) {
  const [picking, setPicking] = useState(false)
  // Standalone (Getting started): the steps own the add-device dialog; the
  // overlay passes its own opener, since the dialog must outlive it.
  const [addDeviceOpen, setAddDeviceOpen] = useState(false)
  const setUpServer = onSetUpServer ?? (() => setAddDeviceOpen(true))
  const { readiness, board } = state
  return (
    <>
      {!onSetUpServer && (
        <ReadinessAddDeviceDialog
          state={state}
          open={addDeviceOpen}
          onOpenChange={setAddDeviceOpen}
        />
      )}
      <div
        className={cn(`flex flex-col divide-y divide-glass-stroke`, className)}
        data-testid="coding-readiness-steps"
      >
        {readiness.steps.map((step) => {
          const current = step.state === `current`
          const showPicker =
            picking && current && step.key === `repository` && board !== null
          return (
            <div
              key={step.key}
              data-step={step.key}
              data-state={step.state}
              className={cn(
                `flex gap-3 px-4 py-3`,
                step.state === `met` ? `items-center` : `items-start`,
                current && `bg-amber-400/[0.06]`
              )}
            >
              <StepGlyph step={step} />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className={cn(
                      `min-w-0 flex-1 truncate text-sm`,
                      step.state === `met`
                        ? `text-muted-foreground`
                        : `font-medium text-foreground`
                    )}
                  >
                    {step.title}
                  </span>
                  {step.detail && (
                    <span className="max-w-[45%] shrink-0 truncate text-xs text-muted-foreground">
                      {step.detail}
                    </span>
                  )}
                </div>
                {step.body && (
                  <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                    {step.body}
                  </p>
                )}
                {showPicker && board ? (
                  <div className="mt-2">
                    <ReadinessRepoPicker
                      teamId={state.teamId}
                      board={board}
                      repos={state.repos}
                      onPicked={() => setPicking(false)}
                      onReload={state.reload}
                    />
                  </div>
                ) : (
                  current && (
                    <FixButtons
                      step={step}
                      state={state}
                      onChooseRepository={() => setPicking(true)}
                      onSetUpServer={setUpServer}
                      onNavigate={onNavigate ?? (() => {})}
                    />
                  )
                )}
              </div>
            </div>
          )
        })}
      </div>
    </>
  )
}

/** "Set up a server" → the Devices page's own Add device dialog. */
function ReadinessAddDeviceDialog({
  state,
  open,
  onOpenChange,
}: {
  state: CodingReadinessState
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <AddDeviceDialog
      open={open}
      onOpenChange={onOpenChange}
      devices={state.ownDevices}
      origin={typeof window === `undefined` ? `` : window.location.origin}
    />
  )
}

/** Green met, amber current, grey pending — one slice per step. */
function ReadinessProgress({ readiness }: { readiness: CodingReadiness }) {
  return (
    <div className="mt-3 flex gap-1" aria-hidden>
      {readiness.steps.map((step) => (
        <span
          key={step.key}
          data-state={step.state}
          className={cn(
            `h-1 flex-1 rounded-full`,
            step.state === `met`
              ? `bg-emerald-400`
              : step.state === `current`
                ? `bg-amber-400`
                : `bg-glass-stroke-strong`
          )}
        />
      ))}
    </div>
  )
}

function StartButton({
  readiness,
  onStart,
  phone,
}: {
  readiness: CodingReadiness
  onStart: () => void
  phone: boolean
}) {
  return (
    <Button
      className="w-full"
      size={phone ? `lg` : `default`}
      variant={readiness.ready ? `default` : `glass`}
      disabled={!readiness.ready}
      onClick={onStart}
      data-testid="coding-readiness-start"
    >
      {phone ? <PlayIcon /> : <MonitorUp />}
      {READINESS_COPY.start}
    </Button>
  )
}

export interface CodingReadinessOverlayProps {
  state: CodingReadinessState
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The footer's Start coding (enabled once ready). */
  onStart: () => void
  /** `auto` = a sheet on phones, the anchored popover elsewhere. */
  surface?: `auto` | `popover` | `sheet`
  /** What the popover hangs under (the capsule + its caption). */
  children?: ReactNode
  align?: `start` | `center` | `end`
}

/** The checklist as a popover anchored under Start coding, or a phone's
 * bottom sheet. */
export function CodingReadinessOverlay({
  state,
  open,
  onOpenChange,
  onStart,
  surface = `auto`,
  children,
  align = `end`,
}: CodingReadinessOverlayProps) {
  const isMobile = useIsMobile()
  const anchorRef = useRef<HTMLDivElement>(null)
  const asSheet = surface === `sheet` || (surface === `auto` && isMobile)
  const { readiness } = state
  const close = () => onOpenChange(false)
  const start = () => {
    close()
    onStart()
  }
  // The add-device dialog outlives the checklist it was opened from.
  const [addDeviceOpen, setAddDeviceOpen] = useState(false)
  const setUpServer = () => {
    close()
    setAddDeviceOpen(true)
  }
  const addDevice = (
    <ReadinessAddDeviceDialog
      state={state}
      open={addDeviceOpen}
      onOpenChange={setAddDeviceOpen}
    />
  )

  if (asSheet) {
    return (
      <>
        {children}
        <Sheet open={open} onOpenChange={onOpenChange}>
          <SheetContent
            side="bottom"
            data-testid="coding-readiness-sheet"
            className="gap-0 p-0 pb-[max(1rem,env(safe-area-inset-bottom))]"
          >
            <SheetHeader className="px-4 pt-3 pb-3">
              <SheetTitle>{READINESS_COPY.title}</SheetTitle>
              <SheetDescription>{readiness.summary}</SheetDescription>
              <ReadinessProgress readiness={readiness} />
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto border-y border-glass-stroke">
              <ReadinessSteps
                state={state}
                onSetUpServer={setUpServer}
                onNavigate={close}
              />
            </div>
            <div className="px-4 pt-4">
              <StartButton readiness={readiness} onStart={start} phone />
            </div>
          </SheetContent>
        </Sheet>
        {addDevice}
      </>
    )
  }

  return (
    <>
      {addDevice}
      <Popover open={open} onOpenChange={onOpenChange}>
        <PopoverAnchor asChild>
          <div ref={anchorRef} className="flex min-w-0 items-center gap-2">
            {children}
          </div>
        </PopoverAnchor>
        <PopoverContent
          // A press on the capsule itself is its own toggle, not "outside".
          onInteractOutside={(event) => {
            if (anchorRef.current?.contains(event.target as Node)) {
              event.preventDefault()
            }
          }}
          align={align}
          sideOffset={8}
          collisionPadding={12}
          className="w-[23.5rem] max-w-[calc(100vw-1.5rem)] overflow-hidden p-0"
          data-testid="coding-readiness-popover"
        >
          <div className="px-4 pt-4 pb-3">
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {READINESS_COPY.title}
                </div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  {readiness.summary}
                </div>
              </div>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={READINESS_COPY.close}
                title={READINESS_COPY.close}
                onClick={close}
              >
                <CloseIcon />
              </Button>
            </div>
            <ReadinessProgress readiness={readiness} />
          </div>
          <div className="max-h-[60vh] overflow-y-auto border-y border-glass-stroke">
            <ReadinessSteps
              state={state}
              onSetUpServer={setUpServer}
              onNavigate={close}
            />
          </div>
          <div className="p-4">
            <StartButton readiness={readiness} onStart={start} phone={false} />
          </div>
        </PopoverContent>
      </Popover>
    </>
  )
}
