import { dockRows, type DockSource } from "@exp/domain-contract/dock"
import { sessionBanner } from "@exp/domain-contract/session-tree"
import cardFixture from "@exp/domain-contract/fixtures/session-card.json"
import { Button } from "./button"
import { DockCard, DockPile, DockSheetBody, DockSheetFooter, DockSheetSpecimenFrame } from "./dock"

// N-12: THE DockCard specimen (fixture `dock.json` ×4): one card, a banner
// with its action, the pile, the fan and the sheet's face, drawn from the
// REAL `DockCard` / `DockPile` / `DockSheetChrome`. The styleguide entry and
// the app's `?dev=dock-card` capture render it; the natives' specimens
// (`dock_card.rs`, iOS `sg_dock-card`, Android `DockCardSpecimenScreen`) draw
// the same rows.

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
// EXP-1374: the session's own state cards (session-card.json `bannerCases`):
// background tasks and (#EXP-1405) queued, waiting for its device (S-3: an
// interruption draws nothing).
const STATE_ROWS = dockRows(
  cardFixture.bannerCases
    .map((entry) => sessionBanner(entry.session as never))
    .filter((banner) => banner !== null)
    .filter((banner, index, all) => all.findIndex((other) => other.id === banner.id) === index)
    .map((banner): DockSource => ({ kind: `banner`, ...banner }))
)
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

export function DockCardSpecimen() {
  return (
    <div data-testid="dock-card-specimen">

      <div className="grid max-w-2xl grid-cols-1 gap-3">
        <Caption>One card: the label, the caption, the meta, the title, the chevron</Caption>
        <DockCard row={QUESTION} onOpen={noop} />
        <Caption>A banner: never opens; its action word trails</Caption>
        <DockCard row={BANNER} action={action(BANNER)} />
        <Caption>The session's own state: background tasks, queued</Caption>
        {STATE_ROWS.map((row) => (
          <DockCard key={row.id} row={row} />
        ))}
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
  
    </div>
  )
}
