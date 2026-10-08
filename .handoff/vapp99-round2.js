export const meta = {
  name: 'exponential-ui-round-2',
  description: 'Exponential UI round 2: open items, every gpui divergence cause fixed (loop until zero), Compose parity, web perf + virtual lists, docs site perfected; reviewed and verified',
  phases: [
    { title: 'Contract', detail: 'round-2 contract items in the catalog + TS reference' },
    { title: 'Implement', detail: 'React ∥ docs site ∥ (Rust core → gpui ∥ Compose)' },
    { title: 'Converge', detail: 'conformance loop: fix divergence causes until zero or dry' },
    { title: 'Review', detail: 'adversarial review → fix → re-check per lane' },
    { title: 'Verify', detail: 'full matrix, shots, completeness critic' },
  ],
}

const ROOT = args && args.root
if (!ROOT) throw new Error('pass args.root = the worktree path')
const BASE = (args && args.base) || 'origin/exp/chat-76b77687'

const RULES = `
GROUND RULES (every lane):
- Work in ${ROOT}. Other agents edit OTHER paths of this worktree concurrently: touch ONLY the paths you own. Never git commit/stash/checkout/reset/clean/rebase/push; the orchestrator commits. git diff/status/log are fine.
- NEVER touch packages/exponential-ui-swift/**, apps/ios/**, apps/desktop/crates/exponential-ui-ffi/build-ios.sh, shots/**/ios.webp or shots/**/desktop.webp: a parallel MacBook run owns iOS and the macOS captures.
- CLAUDE.md stays < 40,000 chars (wc -m); compress, never append; edit only if a rule there became false. Never hand-edit generated files: \`bun run --filter @exponential-at/ui generate\` (also mirrors the Rust generated files); FFI bindings via apps/desktop/crates/exponential-ui-ffi/generate-bindings.sh.
- Env: JS tests with LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8; Rust with CARGO_TARGET_DIR=/tmp/xui-r2-target; gpui samples need RUSTUP_TOOLCHAIN=1.96.0; Android: export ANDROID_HOME=$HOME/Android/Sdk; Kotlin binding suite: JNA_JAR=/usr/share/java/jna-5.14.0.jar bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh kotlin. cargo check --workspace needs the pipewire header workaround (see the desktop memory notes / earlier lane reports: PKG_CONFIG_PATH + BINDGEN_EXTRA_CLANG_ARGS at the extracted pipewire dev headers); if you cannot find it, check only the SDK crates.
- Contract docs: packages/exponential-ui/docs/round-1-contract.md (read it) and the NEW packages/exponential-ui/docs/round-2-contract.md (the contract lane writes it). Fixtures are the ×4 contract: TS reference, Rust core, React, gpui, Compose (and Swift, on the Mac) replay the SAME files.
- Copy rule for everything user-facing (docs site, READMEs, labels): short, clear strings. No filler, no marketing adjectives, no paragraph where a table, a code sample or a picture does it.
- Finish the whole list; for anything truly out of reach say so under "Deferred" with the reason and the smallest next step. Never claim what you did not run.
- Your final text IS the return value: markdown with Done / Contract notes / Deferred / Tests run (commands + results).`

const OPEN_ITEMS = `
ROUND-2 ITEMS (from the round-1 completeness critic + final verification + the integration):
Authoring features: position: sticky (style.json + all renderers; sticky inside scroll containers), backdrop blur (backdropBlur token-only key; web backdrop-filter, gpui/natives best effort with a documented fallback), Resizable panels (native: horizontal/vertical, handles, min/max, bound sizes, keyboard), keyframe animations (a small token-only \`animation\` key: pulse|spin|fade-in|slide-in-{up,down,left,right}|shimmer with motion tokens; reduced-motion aware), number/date/currency/percent/relative-time formatting via a host-supplied Formatter (TS: Intl; Rust: a Formatter trait like Measure with an English fallback; FFI: a foreign Formatter; locale from SurfaceSettings), remaining hard-coded English labels → string ids (React: 'Invalid value' fallback natives/shared.tsx, Composer field label, CodeBlock region label, title-less Dialog label; same audit in gpui and Compose).
Contract decisions to settle in TS AND Rust together, each with fixtures: lift template-referenced nodes out of children (they must not also render in place), duplicate or empty list/Table keys → "#<index>" and Rust item_suffix accumulates for nested templates, implicit textAlign start/end resolved per node direction (Visual gets direction or a resolved align; inner direction: rtl boxes), Switch/label order (decide ONE order ×4: label first, control trailing, like iOS/shadcn settings rows; fix whichever renderer differs), bind_row_slot exposed over FFI (bindRowSlotJson), Rust layout prop resolution goes through resolve_node_props (literal Table rows containing {path} are data), number-valued text props render (as_str → display string everywhere: core reconcile.rs, gpui events/paint).
Virtual lists: horizontal windowing (List direction row), sticky section headers (List sections / a header slot), windowed lists without a bounded height (window against the nearest scroll container or the host viewport), scrollToIndex ×4, a 100,000-row bench per renderer (React jsdom+Chromium, core, gpui headless, Compose JVM) with numbers in the READMEs.
Conformance coverage: data-xui-id (or data-xui-part) on every control part React paints so the 112-122 gpui-only parts per kitchen-sink case are compared; commit Nunito + Fira Code (OFL) fonts for the conformance and drop the substitutes (licence rows!); shots for the 21 round-1 components (specimens + view-catalog entries; web + android captured here, ios + desktop by the Mac run).`

const DIVERGENCES = `
THE 53 GPUI-vs-WEB DIVERGENCE CAUSES (bun run --filter @exponential-at/ui conformance; gated by apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs ratchet over packages/exponential-ui/fixtures/conformance-known.json; run with --details for the per-node list). Known groups: number-valued text 0 px wide on gpui; empty Text gets a line on gpui; Markdown main-md one line taller at 900/1280; Badge 32 px vs 23.5; Pill +3.9; DatePicker/Select row widths; Radio -4, Slider -8..-10, Table -9, CodeBlock -19, AudioPlayer -14 heights; FileUpload +32..+40; Chart heights (bar -48, line -24, donut -24..-44); Video 180 vs 150 (web relies on the browser default: define an explicit contract size instead); Image ignores aspectRatio on gpui; Sparkline with style width never shrinks on gpui; ToggleGroup stretches on gpui; TreeGuides geometry; playful TabBar icon 20 vs 23; overlay trigger rows 46 (web) vs 32 (gpui) incl. DropdownMenu/HoverCard 20 and menu without icon; resp-drawer trigger-width on gpui vs column-filling on web (largest); RTL bidi of trailing punctuation; resp-states line breaks; Switch before/after label; Checkbox label overlapping on web; Slider thumb/track sizes; Input placeholder inset 14 vs 23; accordion trigger "History · 4" vs "History"; image fallbacks differ; carousel off-screen page placed only on web. For EACH: decide which side is RIGHT per the contract (the web is not automatically right: a browser default is not a contract), fix the wrong side (core, gpui or React, plus the TS/Rust fixtures if the contract changes), regenerate the web baseline only for intended web changes, and lower the ratchet. Target: zero origins, or each survivor justified in conformance-known.json with a reason field.`

const CONTRACT_PROMPT = `You are the round-2 CONTRACT lane of Exponential UI (the cross-platform A2UI SDK: catalog + TS reference in packages/exponential-ui; renderers React, Rust core + gpui, SwiftUI, Compose).
${RULES}
YOU OWN: packages/exponential-ui/** (catalog, src, themes, fixtures, docs, scripts, generated, conformance comparator rules) and the Rust generated mirror the generator writes.
${OPEN_ITEMS}
Do the CONTRACT half of every item: catalog/style/tokens/strings/a11y JSON, the TS reference (reducer, macros, dynamic.ts bind pass, style, locale/format, list keys, direction resolution), fixtures (new cases for every decision; keep bound + responsive cases), the prompt (stay inside the budget), specimens for every component incl. the new ones (fixtures/specimens.json is generated), and write docs/round-2-contract.md: every change with a per-renderer checklist (React / Rust core / gpui / SwiftUI / Compose). Also add the conformance contract for Video/Image/Chart/overlay trigger rows (explicit sizes instead of browser defaults) per the divergence list below, so renderers can converge:
${DIVERGENCES}
Run: generate twice (idempotent), bun test, typecheck. The renderers will fail their replays until their lanes land.`

const reactPrompt = (c) => `You are the round-2 REACT lane.
${RULES}
YOU OWN: packages/exponential-ui-react/** (+ apps/web/src/routes/_authenticated/exponential-ui-kitchen-sink.tsx, apps/web/src/components/exponential-ui-specimen.tsx). Contract is frozen for you: read docs/round-2-contract.md first; contract gaps → report, don't edit packages/exponential-ui.
Contract lane report:
${c}
${OPEN_ITEMS}
Do the React half of every item + the web side of the divergence decisions (only where the contract says the web was wrong), add data-xui-id/data-xui-part on every control part, the 100,000-row list bench (jsdom + Chromium) and horizontal/sticky/unbounded windowing, Formatter via Intl, string ids. Rewrite the conformance web baseline (\`bun run --filter @exponential-at/ui conformance -- --write-baseline\`) only after intended changes and say which. Run typecheck, vitest, browser suite, build:css, packages/ui vitest.`

const docsPrompt = (c) => `You are the round-2 DOCS SITE lane: apps/ui-site = ui.exponential.at, the Exponential UI component docs. It exists (VAPP-93): component pages data-driven from docs/components.generated.json with a live React render + prop switcher, A2UI JSON, four platform shots, props table; playground; themes; builder; guides.
${RULES}
YOU OWN: apps/ui-site/**, packages/site-shell/** (keep apps/marketing building), apps/styleguide/** only for links.
Contract lane report:
${c}
GOAL: the best component docs of any UI framework. Study the patterns of shadcn/ui, Radix Themes, Mantine, Chakra, MUI, Apple HIG and Material 3 docs (from memory; WebFetch if available) and apply the best of them:
- Components index = a VISUAL grid: every component as a live mini render (not a name list), grouped (Layout, Text, Media, Navigation, Overlay, Feedback, Data, Actions, Inputs, Lists), search + filter by group, platform availability dots.
- Component page: a big live preview on top with a theme switcher (exponential/neutral/playful), mode, direction (LTR/RTL), width presets (390/768/1280) to SHOW responsiveness; tabs Preview | A2UI JSON | React | SwiftUI | Compose | gpui (how to embed); a platform strip with the 4 real shots side by side at equal height; variants gallery (every variant/size/state as small live renders, not prose); props table (name, type, default, one short line); events + slots; a11y keyboard table from catalog/a11y.json; "Related" chips.
- Copy: one sentence per component max. Remove filler everywhere on the site (home, concepts, guides): cut every sentence that does not carry information; prefer code, tables, live renders, diagrams.
- Every round-1 and round-2 component has a page and specimen; components without a native shot show a clear "shot pending" state, never a broken image.
- Responsive + accessible site itself; Lighthouse 95+ (keep the prerender + SEO engine); keep the playground, themes, builder, guides working; dark/light.
- Tests: the site's tests + build; add a test that every catalog component has a page, a specimen, a description <= 160 chars and no lorem ipsum. Screenshot the main pages (Playwright via packages/exponential-ui-react tooling or the site's own) to /tmp/ui-site-shots/*.png and list them in your report.`

const corePrompt = (c) => `You are the round-2 RUST CORE + FFI lane (apps/desktop/crates/exponential-ui, apps/desktop/crates/exponential-ui-ffi except build-ios.sh).
${RULES}
YOU OWN those two crates and apps/desktop/Cargo.{toml,lock} for deps. Not gpui (its lane starts after you), not packages/**.
Contract lane report:
${c}
${OPEN_ITEMS}
Do the core half of every item: bind pass + layout prop resolution through resolve_node_props, number-valued text as display strings, template lifting, keys + item_suffix, per-node direction/implicit align in Visual, Formatter trait + FFI foreign Formatter, bindRowSlotJson, sticky positioning in layout output (painters need sticky offsets), backdrop blur/animation/resizable in Visual + events, horizontal + sticky-header + unbounded windowing, scrollToIndex, the 100,000-row bench (README numbers). Replay every fixture incl. round 2; regenerate bindings; Kotlin suite; clippy -D warnings; publish --dry-run. Write every API change precisely: the gpui and Compose lanes and the Mac (Swift) run adapt to it.`

const gpuiPrompt = (c, core) => `You are the round-2 GPUI PAINTER lane (apps/desktop/crates/exponential-ui-gpui + apps/desktop/crates/ui/src/exponential_ui_screen.rs). Goal: the desktop renderer becomes perfect: pixel-faithful to the contract and equal to the web wherever the contract says so.
${RULES}
YOU OWN the gpui crate + that screen file; small ADDITIVE core changes allowed if listed (core lane is finished).
Contract lane report:
${c}
Core lane report:
${core}
${OPEN_ITEMS}
${DIVERGENCES}
Implement the gpui half of every item (sticky, backdrop blur best effort, animations, Resizable, Formatter, string ids, direction-aware align, windowing modes, 100k bench) and fix as many divergence causes as you can, lowering the ratchet (EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance) after each verified fix. Report the remaining origins grouped.`

const composePrompt = (c, core) => `You are the round-2 COMPOSE lane (packages/exponential-ui-compose: the Android painter, VAPP-89). It still targets the PRE-round-1 contract.
${RULES}
YOU OWN packages/exponential-ui-compose/** and shots/**/android.webp + their view-catalog entries.
Contract lane report:
${c}
Core lane report (FFI API!):
${core}
Bring Compose to full parity with round 1 AND round 2 (docs/round-1-contract.md + round-2-contract.md): bind pass via the core, the 21 round-1 components + native Table + the round-2 ones, settings (locale, Formatter via java.text/ICU, density, font scale, safe areas), keyboard (hardware) + TalkBack semantics from catalog/a11y.json, overlays as core layers, scroll containers, sticky, windowing modes, animations, RTL. Then join the conformance harness: a Compose dump producer (Robolectric/JVM layout with the conformance fonts, same dump format as packages/exponential-ui/conformance/dump.ts) compared against the web baseline with a ratchet like gpui's; fix divergences. Re-record snapshots intentionally. Capture android shots for every specimen + kitchen sink on the emulator if one boots here (~/Android/Sdk/emulator; else Paparazzi/Roborazzi images) and update the view-catalog/shots per packages/shots rules. Run ./gradlew test (+ connected tests if an emulator runs).`

const LANES = [
  { key: 'contract', title: 'Contract', paths: 'packages/exponential-ui' },
  { key: 'react', title: 'React renderer', paths: 'packages/exponential-ui-react apps/web/src' },
  { key: 'docs', title: 'Docs site', paths: 'apps/ui-site packages/site-shell' },
  { key: 'core', title: 'Rust core + FFI', paths: 'apps/desktop/crates/exponential-ui apps/desktop/crates/exponential-ui-ffi' },
  { key: 'gpui', title: 'gpui painter', paths: 'apps/desktop/crates/exponential-ui-gpui apps/desktop/crates/ui/src/exponential_ui_screen.rs packages/exponential-ui/fixtures/conformance-known.json' },
  { key: 'compose', title: 'Compose painter', paths: 'packages/exponential-ui-compose' },
]

const FINDINGS = { type: 'object', properties: { findings: { type: 'array', items: { type: 'object', properties: { file: { type: 'string' }, line: { type: 'integer' }, severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, summary: { type: 'string' }, failure: { type: 'string' }, fix: { type: 'string' } }, required: ['file', 'severity', 'summary', 'failure', 'fix'] } }, missing_from_scope: { type: 'array', items: { type: 'string' } }, verdict: { type: 'string' } }, required: ['findings', 'missing_from_scope', 'verdict'] }
const CONV = { type: 'object', properties: { origins: { type: 'integer' }, groups: { type: 'array', items: { type: 'object', properties: { cause: { type: 'string' }, cases: { type: 'integer' }, nodes: { type: 'string' }, delta: { type: 'string' } }, required: ['cause', 'cases'] } }, compose_origins: { type: 'integer' }, summary: { type: 'string' } }, required: ['origins', 'groups', 'summary'] }

phase('Contract')
const contract = await agent(CONTRACT_PROMPT, { label: 'contract', phase: 'Contract', model: 'opus', effort: 'xhigh' })
if (!contract) throw new Error('contract lane returned nothing')

phase('Implement')
const [react, docs, native] = await parallel([
  () => agent(reactPrompt(contract), { label: 'impl:react', phase: 'Implement', model: 'opus', effort: 'high' }),
  () => agent(docsPrompt(contract), { label: 'impl:docs', phase: 'Implement', model: 'opus', effort: 'high' }),
  async () => {
    const core = await agent(corePrompt(contract), { label: 'impl:core', phase: 'Implement', model: 'opus', effort: 'high' })
    const [gpui, compose] = await parallel([
      () => agent(gpuiPrompt(contract, core || '(core lane returned nothing)'), { label: 'impl:gpui', phase: 'Implement', model: 'opus', effort: 'high' }),
      () => agent(composePrompt(contract, core || '(core lane returned nothing)'), { label: 'impl:compose', phase: 'Implement', model: 'opus', effort: 'high' }),
    ])
    return { core, gpui, compose }
  },
])
const reports = { contract, react, docs, core: native && native.core, gpui: native && native.gpui, compose: native && native.compose }

phase('Converge')
let last = null
let dry = 0
const convergence = []
for (let round = 1; round <= 6 && dry < 2; round++) {
  const state = await agent(`Run the conformance harness in ${ROOT} (bun run --filter @exponential-at/ui conformance -- --details; plus the Compose dump if the compose lane wired one) and return the current divergence ORIGINS grouped by cause (count of origins, cases affected, sample nodes, typical delta). Read-only: do not edit anything. Env notes:${RULES}`, { label: `measure:${round}`, phase: 'Converge', model: 'opus', effort: 'medium', schema: CONV })
  if (!state) break
  convergence.push({ round, origins: state.origins, compose: state.compose_origins, groups: state.groups.length })
  log(`converge round ${round}: ${state.origins} gpui origins in ${state.groups.length} causes${state.compose_origins != null ? `, compose ${state.compose_origins}` : ''}`)
  if (state.origins === 0 && !state.compose_origins) break
  if (last && state.origins >= last.origins) dry++
  else dry = 0
  last = state
  await parallel(state.groups.slice(0, 24).map((g, i) => () => agent(`Fix ONE conformance divergence cause in Exponential UI (round ${round}, cause ${i + 1}).
${RULES}
CAUSE: ${JSON.stringify(g)}
Find the root cause; decide per round-1/round-2 contract which side is right; fix the wrong side (gpui painter or core; React only if the contract says the web is wrong; Compose if it is a compose cause). You may only touch files needed for THIS cause; other fixers run concurrently on other causes, so keep edits minimal and local, re-read files before editing, and never rewrite shared fixtures wholesale. Do NOT lower the ratchet or rewrite the baseline yourself: the next measuring round does it. Run the narrowest tests that prove the fix (cargo test -p exponential-ui-gpui --test conformance prints the FIX lines). Return what you changed and the evidence.`, { label: `fix:r${round}:${i + 1}`, phase: 'Converge', model: 'opus', effort: 'high' })))
  await agent(`After round ${round} of conformance fixes in ${ROOT}: run the full conformance (web baseline + gpui ratchet + compose if wired). If the web baseline changed only by INTENDED fixes from this round, rewrite it; then lower the ratchet budget (EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance) so improvements are locked. Make sure cargo test -p exponential-ui -p exponential-ui-gpui and the React vitest still pass; fix fallout from concurrent fixers (conflicting edits, broken tests). Report the new totals.${RULES}`, { label: `ratchet:r${round}`, phase: 'Converge', model: 'opus', effort: 'high' })
}
log(`convergence: ${JSON.stringify(convergence)}`)

phase('Review')
const reviewed = await pipeline(
  LANES,
  (lane) => agent(`Adversarial code review (read-only) of the round-2 ${lane.title} lane of Exponential UI. Diff: git -C ${ROOT} diff ${BASE} -- ${lane.paths} plus untracked files under those paths. Verify the lane report's claims by running its tests. Look for: correctness bugs, contract drift between TS/Rust/React/gpui/Compose (same fixture → different result), responsive/RTL/a11y regressions, hard-coded English, perf cliffs, tests that assert nothing, loosened tolerances or ratchets raised, generated files hand-edited, edits outside the lane's paths, filler copy on the docs site. Severity: blocker/major/minor. List missing_from_scope items the lane neither did nor deferred.
Lane report:
${reports[lane.key] || '(none)'}`, { label: `review:${lane.key}`, phase: 'Review', model: 'opus', effort: 'high', schema: FINDINGS }),
  async (review, lane) => {
    if (!review || (!review.findings.length && !review.missing_from_scope.length)) return { lane: lane.key, review, open: null }
    log(`${lane.key}: ${review.findings.length} findings, ${review.missing_from_scope.length} missing`)
    const fixed = await agent(`FIX lane for the round-2 ${lane.title}. Fix every blocker and major and the minors you can; disputes need evidence.${RULES}
YOU OWN: ${lane.paths} (name any other file you had to touch).
Findings: ${JSON.stringify(review, null, 2)}`, { label: `fix:${lane.key}`, phase: 'Review', model: 'opus', effort: 'high' })
    const open = await agent(`Re-check (read-only) the ${lane.title} fixes against these findings; run the tests; return only what is STILL open.
Findings: ${JSON.stringify(review, null, 2)}
Fixer: ${fixed}`, { label: `recheck:${lane.key}`, phase: 'Review', model: 'opus', effort: 'medium', schema: FINDINGS })
    return { lane: lane.key, review, fixed, open }
  },
)

phase('Verify')
const summary = Object.entries(reports).map(([k, v]) => `## ${k}\n${v || '(none)'}`).join('\n\n') + `\n\n## convergence\n${JSON.stringify(convergence)}\n\n## review\n${JSON.stringify(reviewed.filter(Boolean).map((r) => ({ lane: r.lane, open: r.open })), null, 2)}`
const final = await agent(`FINAL VERIFICATION of Exponential UI round 2 in ${ROOT}. Run and fix until green (minimal correct fixes, never delete tests, never loosen ratchets): generate ×2 (idempotent); packages/exponential-ui bun test + typecheck; React typecheck + vitest + browser + build:css; packages/ui vitest; apps/web tsc + context-budget/third-party-licences/changelog tests; apps/ui-site test/typecheck/build + check:guides (swift needs macOS); view-catalog + shots tests + bun run shots:index; cargo test + clippy -D warnings for exponential-ui, -ffi, -gpui; cargo check --workspace; Kotlin binding suite; Compose ./gradlew test; conformance (report table: web, gpui, compose); licence notices if deps changed (bun run notices:collect:rust/npm + notices). The worktree must end with everything ready to commit (do not commit). Then write the round's final report: per lane what landed, the convergence curve, remaining divergences with reasons, deferred items, commands + results.${RULES}
Round summary:
${summary}`, { label: 'verify', phase: 'Verify', model: 'opus', effort: 'high' })
const critic = await agent(`Completeness critic (read-only) for Exponential UI round 2 in ${ROOT}. Check every item below against the TREE (not the prose): landed / partial / deferred (reason) / silently missing, with the proving file. Items:
${OPEN_ITEMS}
${DIVERGENCES}
Docs site goals: visual components index, per-component live preview with theme/mode/direction/width switchers, embed tabs ×4, platform shot strip, variants gallery, props/events/slots/a11y tables, short copy.
Final report:
${final}`, { label: 'critic', phase: 'Verify', model: 'opus', effort: 'high' })

return { reports, convergence, reviewed, final, critic }
