import {
  conceptIcon,
  getActionIcon,
  Badge,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuDot,
  SidebarMenuItem,
  SidebarMenuTrailing,
  SidebarProvider,
  type SidebarDotTone,
} from "@exp/ui"

import { designTokens } from "@exp/design-tokens"

import type { StyleguideEntry } from "./types.ts"

// EXP-1477 — THE sidebar row's trailing column: the REAL `@exp/ui` sidebar
// primitives (the team rail's nav rows, a section header and its agent
// rows). Every trailing indicator — a nav row's dot, the Drafts count, an
// agent's needs-you dot or spinner, a section's "+", a row's hover gear —
// sits in ONE 20px column 4px in from the row's right edge. #EXP-1480: the
// dot (`SidebarMenuDot`, `sidebar.trailingDot`) and the count (`Badge`) are
// the same on web and the IDE.

const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavThreadsIcon = conceptIcon(`nav-threads`)
const NavDraftsIcon = conceptIcon(`nav-drafts`)
const UiAddIcon = conceptIcon(`ui-add`)

const Dot = ({ tone }: { tone: SidebarDotTone }) => (
  <SidebarMenuTrailing className="pointer-events-none">
    <SidebarMenuDot tone={tone} />
  </SidebarMenuTrailing>
)

const AGENTS = [
  { name: `Big Brother`, icon: `eye`, color: `#a855f7`, needsYou: true },
  { name: `Triage`, icon: `bug`, color: `#8b5cf6`, needsYou: false },
] as const

function SidebarRowsSpecimen() {
  return (
    <SidebarProvider className="min-h-0 w-[256px]">
      <div className="w-full rounded-lg bg-sidebar text-sidebar-foreground">
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton density="compact">
                  <NavInboxIcon className="h-4 w-4" />
                  <span>Inbox</span>
                </SidebarMenuButton>
                <Dot tone="unread" />
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton density="compact">
                  <NavThreadsIcon className="h-4 w-4" />
                  <span>Chats</span>
                </SidebarMenuButton>
                <Dot tone="needsYou" />
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton density="compact">
                  <NavDraftsIcon className="h-4 w-4" />
                  <span>Drafts</span>
                </SidebarMenuButton>
                <SidebarMenuTrailing className="pointer-events-none">
                  <Badge count={2} />
                </SidebarMenuTrailing>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
        <SidebarGroup>
          <SidebarGroupLabel>Agents</SidebarGroupLabel>
          <SidebarGroupAction title="New agent" aria-label="New agent">
            <UiAddIcon className="h-4 w-4" />
          </SidebarGroupAction>
          <SidebarGroupContent>
            <SidebarMenu>
              {AGENTS.map((agent) => {
                const Icon = getActionIcon({ icon: agent.icon })
                return (
                  <SidebarMenuItem key={agent.name}>
                    <SidebarMenuButton density="compact">
                      <Icon className="size-4 shrink-0" style={{ color: agent.color }} />
                      <span>{agent.name}</span>
                    </SidebarMenuButton>
                    {agent.needsYou && <Dot tone="needsYou" />}
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </div>
    </SidebarProvider>
  )
}

export const entry: StyleguideEntry = {
  id: `sidebar-rows`,
  section: `general`,
  owner: `EXP-1477`,
  title: `Sidebar rows`,
  blurb: `The team rail's rows: compact 28px nav, agent, app, board and Active rows under muted section headers. Every trailing indicator shares ONE column: 20px wide, 4px in from the row's right edge, contents centred (a count grows leftwards). A nav row's dot, the Drafts count, an agent's needs-you dot or spinner, an Active row's device glyph, a section header's "+" and a row's hover gear (which takes the column from the dot while hovered) all line up in it. The dot is ONE ${designTokens.sidebar.trailingDot}px disc (\`sidebar.trailingDot\`) in the rail's tones: unread = primary, needs you = \`semantic.yellow\` (${designTokens.semantic.yellow}), live = green; the Drafts count is the eUI 16px count badge (\`Badge\` / IDE \`count_badge\`), muted.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SidebarMenuTrailing`,
      file: `packages/ui/src/sidebar.tsx`,
      note: `the team rail (apps/web/src/components/team/sidebar*.tsx)`,
    },
    desktop: {
      state: `ok`,
      symbol: `rail_trailing_slot`,
      file: `apps/desktop/crates/ui/src/sidebar.rs`,
    },
    ios: { state: `n/a`, note: `Phones have no team rail: the tab bar carries the nav dots.` },
    android: { state: `n/a`, note: `Phones have no team rail: the tab bar carries the nav dots.` },
  },
  island: () => <SidebarRowsSpecimen />,
}
