// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs"
import { join, relative } from "node:path"
import { describe, expect, it } from "vitest"
import { ASSISTANT_FAB } from "@exp/domain-contract/agent-settings"
import { FAB_CLEARANCE_CLASS, FAB_FOOT_CLASS, FAB_FOOT_INSET_VAR, FAB_LANE_CLASS } from "./fab-chrome"

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

  it(`carries the round mention pill, unscoped`, () => {
    // Chat e4a04d22: `<MentionPill>` (chat text) and the editors' `@email`
    // decoration share this box; it must reach text outside the editor.
    const block = css.slice(css.indexOf(`\n.mention-pill {`))
    expect(css.indexOf(`\n.mention-pill {`)).toBeGreaterThan(-1)
    expect(block.slice(0, block.indexOf(`}`))).toContain(`border-radius: 9999px`)
  })

  it(`owns every shared utility the app used to declare`, () => {
    // The motion durations, the glass recipes and the full-screen gradient.
    // EXP-961: `emoji-glyph` joined them when the picker moved in — the colour
    // -emoji face is the picker's own, and an app that kept only the component
    // would draw ☺-style outlines.
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
      `emoji-glyph`,
    ]) {
      expect(css, `missing @utility ${name}`).toContain(`@utility ${name} {`)
    }
    // The picker's per-group content-visibility box rides with it.
    expect(css).toContain(`.emoji-picker-section {`)
  })

  // #EXP-1691: `bg-glass-card-opaque` is a two-layer utility, not a colour, so
  // a ring on the card's ground needs this one or it paints currentColor.
  it(`owns the card's ground as a colour`, () => {
    expect(css).toContain(`--color-card-opaque:`)
  })

  // EXP-895: the diff view moved into the package, so the `.diff-code` scope
  // its per-line lowlight output renders under has to move with it — an app
  // that kept only the tiptap block would draw an uncoloured diff.
  it(`owns the .diff-code highlight scope`, () => {
    for (const token of [
      `.diff-code .hljs-comment`,
      `.diff-code .hljs-keyword`,
      `.diff-code .hljs-string`,
      `.diff-code .hljs-number`,
      `.diff-code .hljs-title`,
      `.diff-code .hljs-variable`,
      `.diff-code .hljs-tag`,
      `.diff-code .hljs-deletion`,
      `.diff-code .hljs-addition`,
    ]) {
      expect(css, `missing ${token}`).toContain(`${token}`)
    }
    // The EDITOR's copy of the same palette stays in apps/web with tiptap.
    expect(css).not.toContain(`.tiptap-content`)
  })

  it(`reserves the Assistant FAB's corner while it shows (#EXP-1860)`, () => {
    // agent-settings.json `assistant.fab.clearance`: page scroll content ends
    // with a spacer the corner tall, THE Dock keeps its right edge clear.
    const scope = `main:has(> [data-assistant-fab])`
    expect(css).toContain(`${scope} {\n  --fab-clearance: ${ASSISTANT_FAB.clearance}px;`)
    expect(css).toContain(`${scope} .${FAB_CLEARANCE_CLASS}::after {`)
    expect(css).toContain(`${scope} .${FAB_LANE_CLASS} {`)
  })

  it(`keeps every pinned foot beside the Assistant FAB (#EXP-1872)`, () => {
    // A pane's pinned foot (the plan rail's Back / Next) ends the corner's
    // width in from the card's edge, less the pane's own inset: the rail's
    // Next (inset = its 16px padding) gets 68px, so it ends 84px in, 16px
    // left of the 52px circle.
    const scope = `main:has(> [data-assistant-fab])`
    expect(css).toContain(
      `${scope} .${FAB_FOOT_CLASS} {\n  padding-right: max(0px, calc(var(--fab-clearance) - var(${FAB_FOOT_INSET_VAR}, 0px)));\n}`
    )
  })

  it(`pins nothing to a pane's foot without THE pinned foot (#EXP-1872)`, () => {
    // A control positioned at the bottom of a pane (a floating bulk bar, a
    // footer strip) wears FAB_FOOT_CLASS (or THE Dock's FAB_LANE_CLASS) on the
    // same element, so the layout keeps it beside the Assistant FAB. The
    // allowlist = what can never meet the FAB, with the reason; a NEW pinned
    // element fails here until it wears the foot or earns an entry. (Footers
    // that end a flex column, like the plan rail's, use the same class; the
    // desktop twin is `assistant_fab_live_tests`.)
    const allowed: Record<string, { count: number; why: string }> = {
      "apps/web/src/components/bulk-action-bar.tsx": { count: 1, why: `max-md only: the phone slot (the md+ host wears the foot)` },
      "apps/web/src/components/team/mobile-tab-bar.tsx": { count: 1, why: `phones: no FAB` },
      "apps/web/src/components/team/sidebar.tsx": { count: 1, why: `the sidebar, outside <main>` },
      "apps/web/src/routes/support/$token.tsx": { count: 1, why: `the support page, outside the team <main>` },
      "packages/ui/src/chat-list.tsx": { count: 1, why: `a row's trailing fade, not a pane foot` },
      "packages/ui/src/chat-specimens.tsx": { count: 1, why: `a catalogue specimen (no FAB on specimens)` },
      "packages/ui/src/detail-chrome.tsx": { count: 3, why: `glass edge fades, no control` },
      "packages/ui/src/dock.tsx": { count: 1, why: `THE Dock's band: its column wears FAB_LANE_CLASS` },
      "packages/ui/src/mobile-work-bar.tsx": { count: 1, why: `phones: no FAB` },
      "packages/ui/src/phone-board-menu.tsx": { count: 1, why: `phones: no FAB` },
      "packages/ui/src/pr-row.tsx": { count: 1, why: `a tree guide line, no control` },
      "packages/ui/src/session-results-view.tsx": { count: 2, why: `a tile's scrim + badge inside the tile` },
    }
    const root = join(import.meta.dirname, `..`, `..`, `..`)
    const pinned = /\b(absolute|sticky|fixed)\b[^"`]*\bbottom-[0-9]/
    const found: Record<string, number> = {}
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name)
        if (entry.isDirectory()) {
          if (entry.name !== `node_modules`) walk(path)
        } else if (/\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name)) {
          for (const line of readFileSync(path, `utf8`).split(`\n`)) {
            if (!pinned.test(line) || line.includes(FAB_FOOT_CLASS) || line.includes(`FAB_FOOT_CLASS`) || line.includes(`FAB_LANE_CLASS`)) continue
            const key = relative(root, path)
            found[key] = (found[key] ?? 0) + 1
          }
        }
      }
    }
    walk(join(root, `apps`, `web`, `src`))
    walk(join(root, `packages`, `ui`, `src`))
    expect(found).toEqual(Object.fromEntries(Object.entries(allowed).map(([file, { count }]) => [file, count])))
  })
})
