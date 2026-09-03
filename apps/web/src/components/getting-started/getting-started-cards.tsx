import { useMemo, useState } from "react"
import { Link } from "@tanstack/react-router"
import { BookOpen, CircleCheck, Download, Lock } from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { GlassGroup } from "@/components/ui/glass-rows"
import { Progress } from "@/components/ui/progress"
import {
  DESKTOP_RELEASES_URL,
  desktopDownloadHref,
} from "@/lib/desktop-download"
import { docsUrl } from "@/lib/docs-links"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"
import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"
import type {
  EntryKey,
  EntryState,
} from "@/components/getting-started/getting-started-model"
import { GETTING_STARTED_COPY } from "@/components/getting-started/getting-started-copy"
import {
  CopySnippetButton,
  McpSetupTabs,
} from "@/components/getting-started/mcp-setup-tabs"
import { buildServerInstallSnippet } from "@/components/my-machines"
import { WidgetLauncherPreview } from "@/components/widget-launcher-preview"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import type { Team } from "@/db/schema"

// The in-app "what to do next" checklist (EXP-88, rebuilt dynamic in
// EXP-141, machines/invite goals in EXP-470, action goal in EXP-548): ten
// entries with live completion state, lock/prereq hints, and per-client MCP
// setup tabs. Signals come from the layout-level
// GettingStartedProgressProvider; the pure state rules live in
// getting-started-model.ts, and every string in `getting-started-copy.ts` —
// all four clients render the same titles/descriptions/order, gated by
// `getting-started-copy.test.ts`.

export interface GettingStartedCardsProps {
  team: Team
  teamSlug: string
  layout?: `grid` | `stack`
}

// EXP-548: concepts, not glyphs — the desktop page resolves the same
// registry constants (`crates/ui/src/getting_started.rs`), so the two
// checklists render byte-identical icons.
const ActionCreateIcon = conceptIcon(`action-create`)
const GithubIcon = conceptIcon(`ui-github`)
const TerminalIcon = conceptIcon(`nav-terminal`)
const HelpdeskIcon = conceptIcon(`nav-support`)

const ENTRY_ICONS: Record<EntryKey, LucideIcon> = {
  desktop: conceptIcon(`ui-device`),
  github: GithubIcon,
  invite: conceptIcon(`ui-invite`),
  board: conceptIcon(`nav-boards`),
  coding: TerminalIcon,
  action: ActionCreateIcon,
  server: conceptIcon(`ui-server`),
  widget: conceptIcon(`settings-widget`),
  helpdesk: HelpdeskIcon,
  mcp: conceptIcon(`ui-mcp`),
}

// One-line hints for locked entries, keyed by entry + the step that unlocks
// it (lockedBy from the model).
function lockedHint(entry: EntryKey, lockedBy: EntryKey): string {
  if (entry === `coding` && lockedBy === `desktop`) {
    return `Connect a machine first — coding sessions run on the desktop app or a registered server.`
  }
  if (entry === `coding` && lockedBy === `github`) {
    return `Connect a GitHub repo first. Coding sessions need a repo-backed board.`
  }
  if (entry === `coding` && lockedBy === `board`) {
    return `Create a board with a repository first.`
  }
  if (entry === `action`) {
    return `Connect a machine first — the action creator runs on the desktop app or a registered server.`
  }
  if (entry === `widget`) {
    return `Create a board first. Widget feedback lands there as issues.`
  }
  return `Complete "${GETTING_STARTED_COPY[lockedBy].title}" first.`
}

function GettingStartedCard({
  icon: Icon,
  title,
  description,
  state,
  stepNumber,
  hint,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  // `neutral` = signals still loading: plain number glyph, no locks/checks.
  state: EntryState | `neutral`
  stepNumber: number
  hint?: string
  children?: React.ReactNode
}) {
  const locked = state === `locked`
  return (
    <GlassGroup className={cn(locked && `opacity-60`)}>
      <div className="flex flex-col gap-1.5 p-4">
        <div className="flex items-center gap-2 text-base font-semibold">
          {state === `done` ? (
            <CircleCheck className="size-5 shrink-0 text-green-500" />
          ) : locked ? (
            <Lock className="size-5 shrink-0 p-0.5 text-muted-foreground" />
          ) : (
            <span className="flex size-5 shrink-0 items-center justify-center rounded-full border text-[11px] font-medium text-muted-foreground">
              {stepNumber}
            </span>
          )}
          <Icon className="size-4 shrink-0" />
          <span>{title}</span>
        </div>
        <p className="text-sm text-muted-foreground">
          {locked && hint ? hint : description}
        </p>
      </div>
      {!locked && children && (
        <div className="mt-auto space-y-3 p-4">{children}</div>
      )}
    </GlassGroup>
  )
}

export function GettingStartedCards({
  team,
  teamSlug,
  layout = `grid`,
}: GettingStartedCardsProps) {
  const { loading, entries, done, total } = useGettingStartedProgressContext()

  const [createOpen, setCreateOpen] = useState(false)

  const downloadHref =
    typeof navigator === `undefined`
      ? desktopDownloadHref(``)
      : desktopDownloadHref(navigator.userAgent, navigator.maxTouchPoints)

  const serverSnippet = useMemo(
    () =>
      buildServerInstallSnippet(
        typeof window === `undefined`
          ? `https://app.exponential.at`
          : window.location.origin
      ),
    []
  )

  const bodies: Record<EntryKey, React.ReactNode> = {
    desktop: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <a href={downloadHref} target="_blank" rel="noreferrer">
            <Download className="mr-1.5 size-4" />
            {GETTING_STARTED_COPY.desktop.action}
          </a>
        </Button>
        <Button size="sm" variant="ghost" asChild>
          <a href={DESKTOP_RELEASES_URL} target="_blank" rel="noreferrer">
            All platforms
          </a>
        </Button>
      </div>
    ),

    // EXP-557: every member connects their own GitHub — no role gate.
    github: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <Link
            to="/t/$teamSlug/settings/repositories"
            params={{ teamSlug }}
          >
            <GithubIcon className="mr-1.5 size-4" />
            {GETTING_STARTED_COPY.github.action}
          </Link>
        </Button>
      </div>
    ),

    board: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          {GETTING_STARTED_COPY.board.action}
        </Button>
      </div>
    ),

    coding: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
            <TerminalIcon className="mr-1.5 size-4" />
            {GETTING_STARTED_COPY.coding.action}
          </Link>
        </Button>
        <p className="text-xs text-muted-foreground">
          Or open any issue in the desktop app and press Start coding.
        </p>
      </div>
    ),

    action: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <Link to="/t/$teamSlug/actions" params={{ teamSlug }}>
            <ActionCreateIcon className="mr-1.5 size-4" />
            {GETTING_STARTED_COPY.action.action}
          </Link>
        </Button>
        <p className="text-xs text-muted-foreground">
          "New action" on the Actions page describes it; your agent builds it.
        </p>
      </div>
    ),

    server: (
      <>
        <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs">
          {serverSnippet}
        </pre>
        <div className="flex flex-wrap items-center gap-2">
          <CopySnippetButton
            label={GETTING_STARTED_COPY.server.action}
            text={serverSnippet}
          />
        </div>
      </>
    ),

    invite: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <Link to="/t/$teamSlug/settings/members" params={{ teamSlug }}>
            {GETTING_STARTED_COPY.invite.action}
          </Link>
        </Button>
      </div>
    ),

    widget: (
      <>
        <div className="flex items-center justify-center rounded-md border bg-muted/30 px-3 py-4">
          <WidgetLauncherPreview />
        </div>
        <p className="text-xs text-muted-foreground">
          Create a widget to get your embed snippet, key, and domain allowlist.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" asChild>
            <Link
              to="/t/$teamSlug/settings/widget"
              params={{ teamSlug }}
            >
              {GETTING_STARTED_COPY.widget.action}
            </Link>
          </Button>
        </div>
      </>
    ),

    helpdesk: (
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <Link
            to="/t/$teamSlug/settings/widget"
            params={{ teamSlug }}
          >
            <HelpdeskIcon className="mr-1.5 size-4" />
            {GETTING_STARTED_COPY.helpdesk.action}
          </Link>
        </Button>
      </div>
    ),

    mcp: <McpSetupTabs />,
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        {loading ? (
          <div />
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <span className="text-sm whitespace-nowrap text-muted-foreground">
              {done}/{total} done
            </span>
            <Progress
              value={total > 0 ? (done / total) * 100 : 0}
              className="max-w-48"
            />
          </div>
        )}
        <Button size="sm" variant="outline" asChild>
          <a href={docsUrl()} target="_blank" rel="noreferrer">
            <BookOpen className="mr-1.5 size-4" />
            Read the docs
          </a>
        </Button>
      </div>

      <div
        className={cn(
          layout === `grid`
            ? `grid gap-4 md:grid-cols-2`
            : `flex flex-col gap-4`
        )}
      >
        {entries.map((entry, index) => (
          <GettingStartedCard
            key={entry.key}
            icon={ENTRY_ICONS[entry.key]}
            title={GETTING_STARTED_COPY[entry.key].title}
            description={GETTING_STARTED_COPY[entry.key].description}
            state={loading ? `neutral` : entry.state}
            stepNumber={index + 1}
            hint={
              entry.lockedBy
                ? lockedHint(entry.key, entry.lockedBy)
                : undefined
            }
          >
            {bodies[entry.key]}
          </GettingStartedCard>
        ))}
      </div>

      <CreateBoardDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        team={team}
      />
    </div>
  )
}
