import { useRef, useState, type ReactNode } from "react"
import { Link, useParams } from "@tanstack/react-router"
import { MonitorUp } from "lucide-react"
import {
  Button,
  Pill,
  Popover,
  PopoverAnchor,
  PopoverContent,
  ReadinessFixPill,
  ReadinessFixes,
  ReadinessProgress,
  ReadinessRow,
  ReadinessRows,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  conceptIcon,
  useIsMobile,
  toast,
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
import { openGithubConnect, POPUP_BLOCKED_MESSAGE } from "@/lib/github-connect"
import { GithubRepoPicker } from "@/components/github-repo-picker"
import { AddDeviceDialog } from "@/components/add-device-dialog"
import type { CodingReadinessState } from "@/hooks/use-coding-readiness"

// EXP-1121: the "Ready to code?" checklist. Start coding ALWAYS renders for a
// member; while a step is missing it is a dashed amber capsule, and a click
// opens this checklist — anchored under the capsule on a pointer device, a
// bottom sheet on a phone — with the fix for the current step inline. The
// model + every word come from `lib/coding-readiness.ts` (fixture-locked ×4);
// the rows, fix pills and progress strip are `@exp/ui`'s
// (`readiness-checklist.tsx`, the styleguide specimen); this file wires each
// fix to the surface that already exists for it. SLOP-7: Connect GitHub
// opens the ONE guided page (`/integrations/github`) as a popup over the
// issue — connect, install the app, pick this board's repository — and the
// rows re-probe once focus comes back.

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

export { READINESS_AMBER } from "@exp/ui"

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
    <ReadinessFixes>
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
          // The first fix is the one to press.
          primary: index === 0,
          "data-testid": `readiness-fix-${fix}`,
        }
        if (fix === `choose_repository`) {
          return (
            <ReadinessFixPill key={fix} {...common} onClick={onChooseRepository}>
              {body}
            </ReadinessFixPill>
          )
        }
        if (fix === `set_up_server`) {
          return (
            <ReadinessFixPill key={fix} {...common} onClick={onSetUpServer}>
              {body}
            </ReadinessFixPill>
          )
        }
        if (fix === `get_desktop_app`) {
          return (
            <ReadinessFixPill key={fix} {...common} asChild>
              <a
                href={desktopDownloadHref(userAgent, touchPoints)}
                target="_blank"
                rel="noreferrer"
              >
                {body}
              </a>
            </ReadinessFixPill>
          )
        }
        if (fix === `connect_github`) {
          // The guided page opens as a popup over the issue — the person
          // never leaves it, and the rows re-probe once focus comes back.
          return (
            <ReadinessFixPill
              key={fix}
              {...common}
              onClick={() => {
                if (
                  !openGithubConnect({
                    teamId: state.teamId,
                    boardId: board?.id ?? null,
                  })
                ) {
                  toast.error(POPUP_BLOCKED_MESSAGE)
                }
              }}
            >
              {body}
            </ReadinessFixPill>
          )
        }
        if (!teamSlug) return null
        if (fix === `board_settings` && board) {
          return (
            <ReadinessFixPill key={fix} {...common} asChild>
              <Link
                to="/t/$teamSlug/settings/boards/$boardId"
                params={{ teamSlug, boardId: board.id }}
                onClick={onNavigate}
              >
                {body}
              </Link>
            </ReadinessFixPill>
          )
        }
        if (fix === `open_devices`) {
          return (
            <ReadinessFixPill key={fix} {...common} asChild>
              <Link
                to="/t/$teamSlug/devices"
                params={{ teamSlug }}
                onClick={onNavigate}
              >
                {body}
              </Link>
            </ReadinessFixPill>
          )
        }
        return null
      })}
    </ReadinessFixes>
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
          open={addDeviceOpen}
          onOpenChange={setAddDeviceOpen}
        />
      )}
      <ReadinessRows className={className}>
        {readiness.steps.map((step) => {
          const current = step.state === `current`
          const showPicker =
            picking && current && step.key === `repository` && board !== null
          return (
            <ReadinessRow
              key={step.key}
              stepKey={step.key}
              icon={STEP_ICONS[step.key]}
              state={step.state}
              title={step.title}
              body={step.body}
              detail={step.detail}
            >
              {showPicker && board ? (
                <div className="mt-2">
                  <GithubRepoPicker
                    source="team"
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
            </ReadinessRow>
          )
        })}
      </ReadinessRows>
    </>
  )
}

/** "Set up a server" → the Devices page's own Add device dialog. */
function ReadinessAddDeviceDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <AddDeviceDialog
      open={open}
      onOpenChange={onOpenChange}
      origin={typeof window === `undefined` ? `` : window.location.origin}
    />
  )
}

/** Green met, amber current, grey pending — one slice per step. */
function ReadinessProgressStrip({ readiness }: { readiness: CodingReadiness }) {
  return (
    <ReadinessProgress
      className="mt-3"
      states={readiness.steps.map((step) => step.state)}
    />
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
              <ReadinessProgressStrip readiness={readiness} />
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
            <ReadinessProgressStrip readiness={readiness} />
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
