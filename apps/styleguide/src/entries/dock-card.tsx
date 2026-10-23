import { dockRows, type DockSource } from "@exp/domain-contract/dock"
import { Button, DockCard, DockPile, DockSheetBody, DockSheetFooter, DockSheetSpecimenFrame } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// N-12: THE DockCard (fixture `dock.json` ×4): the floating card of THE
// Dock, collapsed, piled, fanned, and its sheet; drawn from the REAL
// `DockCard` / `DockPile` / `DockSheetChrome`. The ask-dock entry shows the
// plan and question cards on it; this one is the primitive.

const noop = () => {}

const SOURCES: DockSource[] = [
  { kind: `queued`, messages: [{ id: `m1`, text: `also check the watchdog once the root is idle` }, { id: `m2`, text: `then open the PR` }] },
  {
    kind: `tasks`,
    queue: {
      todo: {
        title: `3 to do`,
        items: [
          { label: `Measuring on the Galaxy A54`, description: `2m 35s`, state: `running` },
          { label: `Rerun the web captures`, description: null, state: `pending` },
          { label: `Open the PR with the numbers`, description: null, state: `pending` },
        ],
      },
      completed: { title: `2 completed` },
    },
  },
  { kind: `suggestions`, id: `m4`, label: `3 suggested tasks`, caption: `1 started`, title: `Lazy-load the markdown editor`, open: 2 },
  {
    kind: `banner`,
    id: `wall`,
    icon: `ui-usage`,
    label: `Usage limit`,
    text: `Claude's 5-hour window is used up until 14:00`,
    tone: `warning`,
    action: `Switch account`,
  },
  { kind: `question`, id: `q1`, text: `Ship both fixes in one PR, or split them?`, sessionName: `Dock polish`, createdAt: 30 },
]

const ROWS = dockRows(SOURCES)
const QUESTION = ROWS.find((row) => row.kind === `question`)!
const BANNER = ROWS.find((row) => row.kind === `banner`)!
const action = (row: { id: string }) =>
  row.id === `banner:wall` ? (
    <Button variant="outline" size="sm" className="h-6">
      Switch account
    </Button>
  ) : null

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `dock-card`,
  section: `special`,
  owner: `N-12`,
  title: `Dock card`,
  blurb: `ONE gray ×4: every card, the composer and the sheet wear the opaque card fill under the card hairline. THE Dock is the floating band over the transcript's bottom edge (the one scroll of a session and a thread), in the composer column: the DockCards, then the composer, which never leaves; its measured height pads the transcript. A DockCard collapsed = the AskCard chrome (the kind icon, the tinted label, the muted caption, the trailing meta) over ONE title line and the open chevron (or a banner's action word). Several cards pile like the toasts (toast-stack.json's geometry, anchored at the bottom): the front card whole, the older ones peeking out 14px above it and shrinking 5% per rank, three at most; a hover-capable pointer fans them on hover, a touch fans them (the next touch opens). A tap opens THE sheet: the composer column on wide screens, edge to edge with the grabber on phones, at most 85% of the viewport; a pinned header (icon, label, caption, the content's meta, Close), ONE scrolling body, a pinned footer for the primary actions; it slides up over motion sheet. Kinds: plan, question (ask-dock.json), tasks (THE Queue), queued (the queued messages), suggestions (EXP-1323's suggested tasks while any is open: its sheet lists them with Start and a ⋯ menu, Start all in the footer), banner (a status strip, never expandable). Model = dockRows, dock, dockAutoOpen (fixture dock.json ×4); Exponential UI ships the collapsed card + its sheet as DockCard.`,
  status: {
    web: {
      state: `ok`,
      symbol: `Dock / DockCard / DockSheet`,
      file: `packages/ui/src/dock.tsx`,
      note: `hosts: ThreadDock (session-thread-view.tsx), useSessionDock (apps/web agent-session.tsx)`,
    },
    desktop: {
      state: `ok`,
      symbol: `dock::band / dock_card / sheet`,
      file: `apps/desktop/crates/ui/src/dock.rs`,
      note: `the pile = domain::stack::stack_layout (shared with toast.rs); the sheet = a window layer; hosts: steer_viewer.rs render_session_dock, thread_screen.rs root_dock`,
    },
    ios: {
      state: `ok`,
      symbol: `DockView / DockPile / DockCard / DockSheetChrome`,
      file: `apps/ios/ExpUI/Sources/Dock.swift`,
      note: `model: ExpCore Dock.swift (the pile = ExpCore Stack.layout, shared with ToastStack); the sheet = GlassSheetChrome; hosts: AgentSessionView (SessionDock.swift), ThreadScreen`,
    },
    android: {
      state: `ok`,
      symbol: `DockCards / DockPile / DockCard / DockSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/Dock.kt`,
      note: `the pile = ToastStack.stackLayout with Dock.STACK, the sheet = GlassSheet; specimens DockCardSpecimenScreen (sg_dock-card), AskDockSpecimenScreen (sg_ask-dock)`,
    },
  },
  island: () => (
    <div className="grid max-w-2xl gap-3">
      <Caption>One card: the label, the caption, the meta, the title, the chevron</Caption>
      <DockCard row={QUESTION} onOpen={noop} />
      <Caption>A banner: never opens; its action word trails</Caption>
      <DockCard row={BANNER} action={action(BANNER)} />
      <Caption>Five cards piled: the front one whole, the rest peeking out above it</Caption>
      <DockPile rows={ROWS} expanded={false} onExpandedChange={noop} onOpen={noop} renderAction={action} />
      <Caption>The pile fanned (hover or a tap): every card, 6px apart</Caption>
      <DockPile rows={ROWS} expanded onExpandedChange={noop} onOpen={noop} renderAction={action} />
      <Caption>The sheet: the pinned header, the scrolling body, the pinned footer</Caption>
      <DockSheetSpecimenFrame row={QUESTION} height={300}>
        <DockSheetBody>
          <p>Ship both fixes in one PR, or split them?</p>
          <p className="mt-3 text-xs text-muted-foreground">Choose one</p>
        </DockSheetBody>
        <DockSheetFooter>
          <div className="flex justify-end">
            <Button size="sm">Answer</Button>
          </div>
        </DockSheetFooter>
      </DockSheetSpecimenFrame>
    </div>
  ),
}
