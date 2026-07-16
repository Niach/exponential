import { useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  Check,
  Copy,
  Download,
  MessageSquarePlus,
  Plug,
  TerminalSquare,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DESKTOP_RELEASES_URL,
  desktopDownloadHref,
} from "@/lib/desktop-download"
import { buildMcpServersConfig, buildWidgetSnippet } from "@/lib/widget-snippet"
import { cn } from "@/lib/utils"

// The in-app "what to do next" cards (EXP-88) — the docs' Start coding /
// Feedback widget / MCP sections condensed into actionable components, so a
// new user never has to leave the app. Prose is adapted from the marketing
// docs (apps/marketing/src/DocsPage.tsx §§03-07; content is duplicated by
// design, like desktop-download.ts duplicates marketing's links).

export const WIDGET_PLACEHOLDER_KEY = `expw_YOUR_PUBLIC_KEY`

type CardKey = `coding` | `widget` | `mcp`

export interface GettingStartedCardsProps {
  workspaceSlug: string
  projectIsPublic?: boolean
  canManageWidgets: boolean
  layout?: `grid` | `stack`
}

function CopySnippetButton({
  label,
  text,
}: {
  label: string
  text: string
}) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      size="sm"
      variant="outline"
      onClick={() => {
        void navigator.clipboard.writeText(text)
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1_500)
      }}
    >
      {copied ? (
        <>
          <Check className="mr-1 size-4" /> Copied
        </>
      ) : (
        <>
          <Copy className="mr-1 size-4" /> {label}
        </>
      )}
    </Button>
  )
}

function GettingStartedCard({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  children?: React.ReactNode
}) {
  return (
    <Card className="flex flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Icon className="size-4" />
          {title}
        </CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="mt-auto space-y-3">{children}</CardContent>
    </Card>
  )
}

export function GettingStartedCards({
  workspaceSlug,
  projectIsPublic,
  canManageWidgets,
  layout = `grid`,
}: GettingStartedCardsProps) {
  const origin = typeof window === `undefined` ? `` : window.location.origin
  const downloadHref =
    typeof navigator === `undefined`
      ? desktopDownloadHref(``)
      : desktopDownloadHref(navigator.userAgent, navigator.maxTouchPoints)

  const codingCard = (
    <GettingStartedCard
      key="coding"
      icon={TerminalSquare}
      title="Start coding with Claude"
      description={`The desktop app is a full git IDE and the one client that runs coding sessions: "Start coding" on any issue hands it to Claude on your machine — it plans first, implements, then commits, pushes, and opens the pull request linked back to the issue. You just need git and the claude CLI on your PATH.`}
    >
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" asChild>
          <a href={downloadHref} target="_blank" rel="noreferrer">
            <Download className="mr-1.5 size-4" />
            Download the desktop app
          </a>
        </Button>
        <Button size="sm" variant="ghost" asChild>
          <a href={DESKTOP_RELEASES_URL} target="_blank" rel="noreferrer">
            All platforms
          </a>
        </Button>
      </div>
    </GettingStartedCard>
  )

  const widgetCard = (
    <GettingStartedCard
      key="widget"
      icon={MessageSquarePlus}
      title="Collect feedback from your site"
      description={`Embed a feedback button on any website. Visitors report a bug with an annotatable screenshot, and it lands here as an issue — with reporter email and page context attached.`}
    >
      <pre className="max-h-40 overflow-auto rounded-md border bg-muted/30 p-3 text-left text-xs">
        {buildWidgetSnippet(WIDGET_PLACEHOLDER_KEY, origin)}
      </pre>
      <p className="text-xs text-muted-foreground">
        {canManageWidgets
          ? `Create a widget to get your real key and domain allowlist.`
          : `A team owner can create widgets in Team Settings to get a real key.`}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        {canManageWidgets && (
          <Button size="sm" asChild>
            <Link
              to="/t/$workspaceSlug/settings/widget"
              params={{ workspaceSlug }}
            >
              Create a widget
            </Link>
          </Button>
        )}
        <CopySnippetButton
          label="Copy snippet"
          text={buildWidgetSnippet(WIDGET_PLACEHOLDER_KEY, origin)}
        />
      </div>
    </GettingStartedCard>
  )

  const mcpCard = (
    <GettingStartedCard
      key="mcp"
      icon={Plug}
      title="Connect your tools via MCP"
      description={`This instance exposes an MCP server at /api/mcp. Point Claude Code, Cursor, or any MCP-aware tool at it to list, create, and edit issues, projects, labels, and comments right from your editor.`}
    >
      <pre className="overflow-auto rounded-md border bg-muted/30 p-3 text-left text-xs">
        {buildMcpServersConfig(origin)}
      </pre>
      <p className="text-xs text-muted-foreground">
        Authenticate via OAuth (the standard MCP flow) or a personal API key as
        a bearer token for headless agents.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <CopySnippetButton
          label="Copy config"
          text={buildMcpServersConfig(origin)}
        />
      </div>
    </GettingStartedCard>
  )

  const cards: Record<CardKey, React.ReactNode> = {
    coding: codingCard,
    widget: widgetCard,
    mcp: mcpCard,
  }
  const order = gettingStartedCardOrder(projectIsPublic)

  return (
    <div
      className={cn(
        layout === `grid`
          ? `grid gap-4 md:grid-cols-3`
          : `flex flex-col gap-4`
      )}
    >
      {order.map((key) => cards[key])}
    </div>
  )
}

// A public feedback board's most likely next step is the widget, so it leads
// there; everywhere else the coding loop comes first. Exported for the unit
// test.
export function gettingStartedCardOrder(projectIsPublic?: boolean): CardKey[] {
  return projectIsPublic
    ? [`widget`, `coding`, `mcp`]
    : [`coding`, `widget`, `mcp`]
}
