import { describe, expect, it } from "vitest"
import { MAIN_PANEL_CLASS } from "@/components/team/app-shell"

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
  // descendants, and the fullscreen AgentDock (`fixed inset-0`) has to escape
  // it. Every one of these creates that containing block.
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
          name.startsWith(banned) ? `${token} traps the fullscreen dock` : token
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
    expect(MAIN_PANEL_CLASS).toContain(`md:m-[10px]`)
    expect(MAIN_PANEL_CLASS).toContain(`md:h-[calc(100dvh-20px)]`)
    expect(MAIN_PANEL_CLASS).toContain(`md:min-h-0`)
  })
})
