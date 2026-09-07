import { describe, expect, it } from "vitest"
import {
  DETAIL_STICKY_BAND_CLASS,
  DOCK_BAND_CLASS,
  MAIN_COLUMN_CLASS,
  MAIN_OUTLET_CLASS,
  MAIN_PANEL_CLASS,
  mainPanelClass,
} from "@/components/team/app-shell"

const TOKENS = MAIN_PANEL_CLASS.split(/\s+/).filter((token) => token.length > 0)

/** The utility name with any variant prefixes (`md:`, `hover:`) stripped off. */
function base(token: string): string {
  const at = token.lastIndexOf(`:`)
  return at < 0 ? token : token.slice(at + 1)
}

describe(`MAIN_PANEL_CLASS`, () => {
  // The card exists only from `md` up: a phone runs full-bleed under the
  // floating tab bar, and a margin/radius/border there would just eat the
  // reading width (EXP-723).
  it(`gates every card property behind md:`, () => {
    const CARD_PROPERTY = [`m-`, `h-[`, `rounded`, `border`, `bg-`, `overflow-`]
    for (const token of TOKENS) {
      const name = base(token)
      const isCard = CARD_PROPERTY.some((prefix) => name.startsWith(prefix))
      if (!isCard) continue
      expect(token.startsWith(`md:`) ? token : `${token} must be md:-gated`).toBe(
        token
      )
    }
    // Sanity: the list above actually matched something.
    expect(TOKENS.some((token) => token.startsWith(`md:`))).toBe(true)
  })

  // The panel would become the containing block for `position: fixed`
  // descendants, and every full-viewport overlay the panel hosts (`fixed
  // inset-0`) has to escape it. Every one of these creates that containing
  // block.
  it(`never creates a containing block for fixed children`, () => {
    for (const token of TOKENS) {
      const name = base(token)
      for (const banned of [
        `transform`,
        `translate-`,
        `scale-`,
        `rotate-`,
        `filter`,
        `blur-`,
        `backdrop-`,
        `will-change-`,
        `contain-`,
        `perspective`,
      ]) {
        expect(
          name.startsWith(banned) ? `${token} traps fixed descendants` : token
        ).toBe(token)
      }
    }
  })

  // Flex children default to `min-width: auto`, so without these a wide
  // descendant (a table, a code block) widens the whole page.
  it(`keeps the column a min-sized flex child`, () => {
    expect(TOKENS).toContain(`min-w-0`)
    expect(TOKENS).toContain(`flex-1`)
    expect(TOKENS).toContain(`flex-col`)
  })

  // Px literals on purpose: the md+ root font is 1.15625rem, so rem-based
  // spacing steps would not line up with the 10px inset the panel is designed
  // around (styles.css L7-15).
  it(`sizes the card in px, not rem steps`, () => {
    expect(MAIN_PANEL_CLASS).toContain(`md:mx-[10px]`)
    expect(MAIN_PANEL_CLASS).toContain(`md:mt-[10px]`)
    expect(MAIN_PANEL_CLASS).toContain(`md:min-h-0`)
  })

  // EXP-771: the column owns the viewport height now, so the card is a
  // `flex-1` child of it. A fixed height here would ignore the dock band and
  // push it off the bottom edge.
  it(`leaves the height to the column`, () => {
    expect(MAIN_PANEL_CLASS).not.toContain(`h-[calc(`)
    expect(MAIN_PANEL_CLASS).not.toContain(`h-dvh`)
    expect(TOKENS).toContain(`flex-1`)
  })
})

describe(`mainPanelClass`, () => {
  // 6px with the band, 10px without — the band is the card's own chrome, so
  // it rides closer than the window inset; with no band the card is inset
  // symmetrically on all four sides again.
  it(`tightens the bottom margin only when the dock is up`, () => {
    expect(mainPanelClass(true)).toContain(`md:mb-[6px]`)
    expect(mainPanelClass(true)).not.toContain(`md:mb-[10px]`)
    expect(mainPanelClass(false)).toContain(`md:mb-[10px]`)
    expect(mainPanelClass(false)).not.toContain(`md:mb-[6px]`)
  })

  // One `mb-` utility per variant, never an `m-[10px]` shorthand a longhand
  // has to beat: Tailwind's shorthand/longhand emit order is not a contract.
  it(`never leans on shorthand-over-longhand ordering`, () => {
    for (const docked of [true, false]) {
      expect(mainPanelClass(docked)).not.toContain(`md:m-[`)
    }
  })

  // Everything the card invariants above pin has to hold for BOTH variants —
  // they are the same card.
  it(`keeps the card contract in both states`, () => {
    for (const docked of [true, false]) {
      const tokens = mainPanelClass(docked).split(/\s+/)
      expect(tokens).toContain(`min-w-0`)
      expect(tokens).toContain(`md:rounded-xl`)
      expect(mainPanelClass(docked)).not.toContain(`backdrop-`)
    }
  })
})

describe(`MAIN_COLUMN_CLASS`, () => {
  const COLUMN_TOKENS = MAIN_COLUMN_CLASS.split(/\s+/).filter(
    (token) => token.length > 0
  )

  // The column takes the card's old slot in the SidebarProvider ROW, so
  // `flex-1`/`min-w-0` are about width; `flex-col` is what stacks the card
  // over the dock band.
  it(`is the row's min-sized column`, () => {
    expect(COLUMN_TOKENS).toContain(`flex-1`)
    expect(COLUMN_TOKENS).toContain(`flex-col`)
    expect(COLUMN_TOKENS).toContain(`min-w-0`)
  })

  // The height moved HERE from the card: the column is the viewport, the card
  // takes what the band leaves. On phones there is no band and no card, so
  // the window keeps scrolling — hence the `md:`.
  it(`owns the viewport height from md up`, () => {
    expect(COLUMN_TOKENS).toContain(`md:h-dvh`)
    expect(COLUMN_TOKENS).not.toContain(`h-dvh`)
  })
})

describe(`DOCK_BAND_CLASS`, () => {
  const BAND_TOKENS = DOCK_BAND_CLASS.split(/\s+/).filter(
    (token) => token.length > 0
  )

  // EXP-771: the band is OUTSIDE the card, on the bare page ground. A fill or
  // a border would make it read as a second card stacked under the first.
  it(`paints nothing of its own`, () => {
    for (const token of BAND_TOKENS) {
      const name = base(token)
      for (const banned of [`bg-`, `border`, `rounded`, `shadow`, `glass-`]) {
        expect(name.startsWith(banned) ? `${token} paints the band` : token).toBe(
          token
        )
      }
    }
  })

  // 36px band, chips 8px inside the card's left edge (the card's own 10px
  // side margin plus the band's 8px padding). Px literals for the same reason
  // the card uses them: `h-9` is 41.6px at the md+ root font, `px-2` 9.25px.
  it(`sizes the band in px and shares the card's side margins`, () => {
    expect(BAND_TOKENS).toContain(`h-[36px]`)
    expect(BAND_TOKENS).toContain(`mx-[10px]`)
    expect(BAND_TOKENS).toContain(`px-[8px]`)
  })

  // A grown band would eat into the card instead of the card shrinking, and
  // a wrapping row of tabs would push the layout around.
  it(`never grows or shrinks the column`, () => {
    expect(BAND_TOKENS).toContain(`shrink-0`)
    expect(BAND_TOKENS).toContain(`overflow-x-auto`)
  })
})

describe(`MAIN_OUTLET_CLASS`, () => {
  const OUTLET_TOKENS = MAIN_OUTLET_CLASS.split(/\s+/).filter(
    (token) => token.length > 0
  )

  // The panel clips at a definite height from `md` up, so the window never
  // scrolls there. Without a scroller on the Outlet wrapper every route that
  // relies on page scroll (all of settings/*) is cut off at the panel's bottom
  // edge.
  it(`is the panel's scrollport from md up`, () => {
    expect(OUTLET_TOKENS).toContain(`md:overflow-y-auto`)
  })

  // A phone scrolls the window, and the panel column grows with the page —
  // a scroller there would trap the content in a nested viewport.
  it(`leaves the phone layout on window scroll`, () => {
    expect(OUTLET_TOKENS).not.toContain(`overflow-y-auto`)
  })

  // A definite-height, min-sized flex child: this is what lets a route with
  // its own `h-full` scroller fill the wrapper exactly instead of
  // double-scrolling it, and keeps a wide table from widening the page.
  it(`stays a min-sized flex child`, () => {
    expect(OUTLET_TOKENS).toContain(`flex-1`)
    expect(OUTLET_TOKENS).toContain(`min-h-0`)
    expect(OUTLET_TOKENS).toContain(`min-w-0`)
  })
})

describe(`DETAIL_STICKY_BAND_CLASS`, () => {
  const BAND_TOKENS = DETAIL_STICKY_BAND_CLASS.split(/\s+/).filter(
    (token) => token.length > 0
  )

  // The band is what keeps the title AND the properties on screen while a long
  // description scrolls (EXP-760, IDE parity) — without `sticky top-0` it is
  // just a header again.
  it(`pins to the top of the detail scroller`, () => {
    expect(BAND_TOKENS).toContain(`sticky`)
    expect(BAND_TOKENS).toContain(`top-0`)
    expect(BAND_TOKENS).toContain(`z-10`)
  })

  // `glass-chrome-top` is the WINDOW-edge scrim; inside the cutout panel it
  // composites a visibly darker rectangle over the card (the black-bar bug),
  // because the panel's own `--glass-fill-panel` layer is missing from it.
  it(`uses the panel-aware scrim, not the window-edge one`, () => {
    expect(BAND_TOKENS).toContain(`glass-chrome-card`)
    expect(BAND_TOKENS).not.toContain(`glass-chrome-top`)
  })

  // The band blurs, so it becomes a containing block for `position: fixed`
  // descendants — which is precisely why the PANEL must not (see above). The
  // pairing is the invariant worth pinning: the blur lives here, never there.
  it(`keeps the blur off the panel`, () => {
    expect(MAIN_PANEL_CLASS).not.toContain(`backdrop-`)
    expect(MAIN_PANEL_CLASS).not.toContain(`glass-chrome`)
  })
})
