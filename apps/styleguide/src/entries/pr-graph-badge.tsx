import { escapeHtml, svgBoxes, svgCircleX, svgLayers, svgWorkflow } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1079: the work header's graph badge — the ONE pill that says what a
// piece of work is PART OF, and the popover behind it.
//
// Hand-drawn, like the session tree it opens onto: the real `PrGraphBadge`
// reads four Electric collections, and a specimen that needs a team documents
// nothing. What the page has to show is the RULE — which concept the pill
// wears for which relation, and that beside the face toggle it stands at the
// toggle's own height (the desktop's `pr_graph::badge_size`, the web's
// `placement="header"`), where it used to be a 24px chip next to a 36px Stop.

/** The four things the badge can say. `runs` has no label: the tree alone. */
function badge(glyphs: string[], label: string | null, placement: `header` | `chip`): string {
  return [
    `<button class="cmp-pill" data-size="${placement === `header` ? `md` : `sm`}"`,
    ` data-mode="action" data-placement="${placement}" type="button">`,
    glyphs.join(``),
    label === null ? `` : `<span class="label">${escapeHtml(label)}</span>`,
    `</button>`,
  ].join(``)
}

/** The Stop pill the badge stands beside — the same rung, the same recipe. */
function stop(placement: `header` | `chip`): string {
  return [
    `<button class="cmp-pill cmp-pr-graph-stop" data-size="${placement === `header` ? `md` : `sm`}"`,
    ` data-mode="action" data-placement="${placement}" type="button">`,
    svgCircleX,
    `<span class="label">Stop</span>`,
    `</button>`,
  ].join(``)
}

/** The face toggle, 36 tall — what the header rung is measured against. */
const TOGGLE = [
  `<div class="cmp-pr-graph-toggle" role="tablist">`,
  `<span class="cmp-pr-graph-segment" data-active>Run</span>`,
  `<span class="cmp-pr-graph-segment">Changes</span>`,
  `</div>`,
].join(``)

function cluster(placement: `header` | `chip`, glyphs: string[], label: string | null): string {
  return [
    `<div class="cmp-pr-graph-cluster" data-placement="${placement}">`,
    badge(glyphs, label, placement),
    placement === `header` ? TOGGLE : ``,
    stop(placement),
    `</div>`,
  ].join(``)
}

/** The popover's rows — the session tree's own row vocabulary (EXP-996). */
function run(depth: number, title: string, live: boolean): string {
  return [
    `<div class="cmp-session-tree-row" data-depth="${depth}">`,
    `<span class="cmp-session-tree-dot"${live ? ` data-live` : ``}></span>`,
    `<span class="cmp-session-tree-title">${escapeHtml(title)}</span>`,
    `</div>`,
  ].join(``)
}

function issueRow(identifier: string, title: string): string {
  return [
    `<div class="cmp-session-tree-row" data-depth="0">`,
    `<span class="cmp-session-tree-id">${escapeHtml(identifier)}</span>`,
    `<span class="cmp-session-tree-title">${escapeHtml(title)}</span>`,
    `</div>`,
  ].join(``)
}

const POPOVER = [
  `<div class="cmp-pr-graph-popover">`,
  `<div class="cmp-pr-graph-section">`,
  `<span class="cmp-pr-graph-caption">Runs</span>`,
  run(0, `Chat`, true),
  run(1, `Plants overview + plant detail pages`, false),
  run(1, `Indexer: trades, bundles, offers, zap volume`, true),
  `</div>`,
  `<div class="cmp-pr-graph-section">`,
  `<span class="cmp-pr-graph-caption">In batch with</span>`,
  issueRow(`STRA-43`, `Indexer: trades, bundles, offers, zap volume`),
  issueRow(`STRA-45`, `Indexer query API for the web app`),
  `</div>`,
  `</div>`,
].join(``)

export const entry: StyleguideEntry = {
  id: `pr-graph-badge`,
  section: `special`,
  owner: `EXP-1079`,
  title: `Work header badge`,
  blurb: `The ONE pill in the work header that says what this work is PART OF, and the popover behind it (EXP-897 §4). The glyph is a concept, never a raw icon: pr-stack (layers) with \`2 of 3\` for a stacked pull request, pr-batch (boxes) with \`3 issues\` for a pull request that closes several, both for a batch inside a stack, and session-tree (the workflow glyph) alone on the Run face of a run that started or was started by other runs. Absent when there is nothing to say. Beside the face toggle it wears the toggle's own 36px rung with a 16px glyph, exactly like Stop, Resume and Merge (EXP-926: the placement is the only thing that sizes a header action); in the phone bars and list headers it is the 24px chip. The popover lists the face's section with the session tree's row vocabulary: Runs on the Run face, Blocked by and In batch with on the Issue face, the pull request stack bottom-up on Changes.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/web/src/components/pr-graph-badge.tsx`,
      note: `placement="header" in the md+ work header; the shape rule is lib/pr-graph.ts badgeShape`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_graph::badge`,
      file: `apps/desktop/crates/ui/src/pr_graph.rs`,
      note: `badge_size() = work_header::header_action_size(false); glyphs from registry::SESSION_TREE / PR_STACK / PR_BATCH`,
    },
    ios: {
      state: `leftover`,
      symbol: `PrGraphBadge`,
      file: `apps/ios/Exponential/UI/Work/PrGraphBadge.swift`,
      note: `chip-scale in the Work bar by design; no session-tree pill on the Run face yet`,
    },
    android: {
      state: `leftover`,
      symbol: `PrGraphBadge`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/PrGraphBadge.kt`,
      note: `chip-scale in the Work bar by design; no session-tree pill on the Run face yet`,
    },
  },
  render: () =>
    [
      `<div class="cmp-pr-graph">`,
      // The header rung: every badge state beside the toggle and Stop.
      cluster(`header`, [svgWorkflow], null),
      cluster(`header`, [svgLayers], `2 of 3`),
      cluster(`header`, [svgBoxes], `3 issues`),
      cluster(`header`, [svgLayers, svgBoxes], `2 of 3`),
      // The chip, where the bar is chip-scale.
      cluster(`chip`, [svgBoxes], `2 issues`),
      // What the Run face's pill opens.
      POPOVER,
      `</div>`,
    ].join(``),
}
