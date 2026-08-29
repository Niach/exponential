import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { useRouterState } from "@tanstack/react-router"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { GettingStartedCards } from "@/components/getting-started/getting-started-cards"
import { ActionSuggestionsPanel } from "@/components/action-suggestions-list"
import type { Team } from "@/db/schema"
import { Button } from "@/components/ui/button"
import { conceptIcon } from "@/lib/icons.generated"

// EXP-686: the Getting started sheet moved out of the sidebar button so the
// Actions/Automations lightbulb can open it too — including once the checklist
// is complete and the sidebar entry has hidden itself. Two tabs: the EXP-88
// checklist, and the action suggestion seeds that left the Actions surface.
export type GettingStartedTab = `first-steps` | `suggestions`

export interface GettingStartedSheetValue {
  /** Opens the sheet on `tab` (default: the checklist). */
  open: (tab?: GettingStartedTab) => void
}

const GettingStartedSheetContext = createContext<GettingStartedSheetValue>({
  open: () => {},
})

/** Never null — outside a team layout (styleguide, tests) opening is a no-op. */
export function useGettingStartedSheet(): GettingStartedSheetValue {
  return useContext(GettingStartedSheetContext)
}

export function GettingStartedSheetProvider({
  teamSlug,
  team,
  children,
}: {
  teamSlug: string
  team: Team | null | undefined
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<GettingStartedTab>(`first-steps`)

  // Close when a card's link navigates (e.g. "Set up in team settings") —
  // otherwise the sheet would keep covering the new page.
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  useEffect(() => {
    setOpen(false)
  }, [pathname])

  const value = useMemo<GettingStartedSheetValue>(
    () => ({
      open: (next = `first-steps`) => {
        setTab(next)
        setOpen(true)
      },
    }),
    []
  )

  return (
    <GettingStartedSheetContext.Provider value={value}>
      {children}
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent side="right" className="overflow-y-auto sm:max-w-xl">
          <SheetHeader>
            <SheetTitle>Getting started</SheetTitle>
            <SheetDescription>
              Set up the coding loop, collect feedback from your site, and
              connect your tools.
            </SheetDescription>
          </SheetHeader>
          <div className="px-4 pb-6">
            {/* Both tabs need the team row (signals, permissions and the
                steer devices derive from it) — while it syncs, the sheet
                chrome alone is fine. */}
            {team && (
              <Tabs
                value={tab}
                onValueChange={(next) => setTab(next as GettingStartedTab)}
              >
                <TabsList className="w-full">
                  <TabsTrigger value="first-steps" className="flex-1">
                    First steps
                  </TabsTrigger>
                  <TabsTrigger value="suggestions" className="flex-1">
                    Suggested actions
                  </TabsTrigger>
                </TabsList>
                <TabsContent value="first-steps">
                  <GettingStartedCards
                    team={team}
                    teamSlug={teamSlug}
                    layout="stack"
                  />
                </TabsContent>
                <TabsContent value="suggestions">
                  <ActionSuggestionsPanel team={team} />
                </TabsContent>
              </Tabs>
            )}
          </div>
        </SheetContent>
      </Sheet>
    </GettingStartedSheetContext.Provider>
  )
}

// EXP-530: the suggestion glyph is a cross-client concept, never a raw glyph.
const ActionSuggestionIcon = conceptIcon(`action-suggestion`)

/** The icon-only lightbulb next to a page's "New …" button (EXP-686) — it
 * opens Getting started on its Suggested actions tab. */
export function SuggestionsButton() {
  const sheet = useGettingStartedSheet()
  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-6 text-muted-foreground"
      aria-label="Suggestions"
      title="Suggestions"
      onClick={() => sheet.open(`suggestions`)}
    >
      <ActionSuggestionIcon className="size-3.5" />
    </Button>
  )
}
