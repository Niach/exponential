// EXP-375 — the wire format between the four ecosystem COLLECTORS and the one
// MERGE step that renders NOTICES.txt.
//
// The split exists because collecting needs toolchains (cargo, node_modules, a
// gradle resolve, network access to pinned Swift revisions) while the drift
// gate has to run in the pure-bun `web` CI job. So collectors write committed
// `inventory/*.json`, and `scripts/generate.ts` is a pure function of those
// files plus `curated/`.
//
// Every collector MUST emit components sorted by (name, version) using
// codepoint order — never `localeCompare`, whose result depends on the host's
// ICU build and would make macOS and ubuntu runners disagree.

/** One reproduced licence body attached to a component. */
export interface LicenceText {
  /** SPDX id this body is the text of (e.g. `MIT`). */
  spdx: string
  /**
   * Where the body came from, for auditability: a filename inside the package
   * (`LICENSE-MIT`), a URL, or `spdx-template` when the package shipped no
   * licence file and the canonical template in `texts/spdx/` was used.
   */
  source: string
  /** The licence text, verbatim. Newlines normalised to `\n`. */
  body: string
}

/** One third-party component in one client's distributed dependency graph. */
export interface Component {
  /** Package name as its ecosystem spells it. */
  name: string
  /** Resolved version. */
  version: string
  /** The SPDX expression exactly as the package declares it. */
  declared: string
  /**
   * The ids whose terms actually bind us, after applying the election policy:
   * every conjunct of an `AND`, exactly one branch of an `OR`. Sorted.
   */
  licenses: string[]
  /** Set iff `declared` contained an `OR` — records which branch was elected. */
  election?: string
  /** Project URL, when the package declares one. */
  homepage?: string
  /**
   * Copyright lines carried by the component. Printed per component even when
   * its licence body is shared with others — MIT/BSD/ISC require the copyright
   * notice, not just the permission text.
   */
  copyright?: string[]
  /** Reproduced licence bodies, one per entry in `licenses`. */
  texts: LicenceText[]
  /**
   * Rust only: the target triples from `deny.toml` whose resolved graph
   * contains this crate. Absent means "all of them".
   */
  platforms?: string[]
  /**
   * Rust only: reachable ONLY through build-dependency edges, so its code runs
   * at build time and is never linked into the shipped binary. Still
   * attributed — but MPL-2.0 §3.2 source availability does not apply to it,
   * which is why `cbindgen` is not in the source-availability section.
   */
  buildOnly?: true
}

/** One collector's output file. */
export interface Inventory {
  /** Always the same sentence — these files are generated, never hand-edited. */
  $comment: string
  ecosystem: `rust` | `npm` | `swift` | `android`
  /** Which distributed client this inventory feeds. */
  scope: `desktop` | `web` | `marketing` | `ios` | `android`
  /** What produced it, for the header of the rendered notice. */
  collector: string
  components: Component[]
}

export const INVENTORY_COMMENT = `AUTO-GENERATED — do not edit. See packages/licenses/README.md.`

/** Codepoint comparison. Deliberately not `localeCompare` (host ICU differs). */
export const byCodepoint = (a: string, b: string): number =>
  a < b ? -1 : a > b ? 1 : 0

export const compareComponents = (a: Component, b: Component): number =>
  byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version)
