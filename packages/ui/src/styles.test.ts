// @vitest-environment node
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

// EXP-887 — the stylesheet half of the package's contract. `styles.css` is the
// theme every consumer imports; these are the pieces an app (or an island
// renderer) would silently lose if a refactor moved one of them back out.

const css = readFileSync(join(import.meta.dirname, `styles.css`), `utf8`)

describe(`@exp/ui styles.css`, () => {
  it(`imports Tailwind exactly once and scans its own sources`, () => {
    // The consuming app imports THIS file instead of `tailwindcss`, so the
    // import lives here — and because the package ships raw .tsx that no app's
    // automatic content detection reaches, the @source line is what makes the
    // shadcn set's utilities exist at all.
    expect(css.match(/@import "tailwindcss";/g)).toHaveLength(1)
    expect(css).toContain(`@source "./**/*.{ts,tsx}";`)
  })

  it(`declares the theme on :root AND :host`, () => {
    // `:host` so the variables survive inside a shadow root — Tailwind v4 emits
    // its own `@theme` vars to `:root, :host`, and a theme that only reached
    // `:root` would leave a shadow-rooted island with no colours at all.
    expect(css).toMatch(/:root,\s*\n:host\s*\{/)
    expect(css).toContain(`.dark {`)
    expect(css).toContain(`@theme inline {`)
  })

  it(`carries the issue chip's 6px rect, not a capsule`, () => {
    // EXP-423/EXP-885: the box `<IssueChip>` and the editor's `#IDENT`
    // decoration share. The app's components/issue-chip.test.tsx locks the
    // other half (that the decoration carries the same class).
    const block = css.slice(css.indexOf(`\n.issue-chip {`))
    expect(block.slice(0, block.indexOf(`}`))).toContain(`border-radius: 6px`)
  })

  it(`owns every shared utility the app used to declare`, () => {
    // The motion durations, the glass recipes and the full-screen gradient.
    // App-only utilities (`emoji-glyph`) deliberately stay in apps/web.
    for (const name of [
      `duration-fast`,
      `duration-standard`,
      `duration-slow`,
      `glass-panel`,
      `bg-glass-card-opaque`,
      `glass-chrome-top`,
      `glass-chrome-card`,
      `glass-chrome-bottom`,
      `bg-app-gradient`,
    ]) {
      expect(css, `missing @utility ${name}`).toContain(`@utility ${name} {`)
    }
    expect(css).not.toContain(`@utility emoji-glyph`)
  })
})
