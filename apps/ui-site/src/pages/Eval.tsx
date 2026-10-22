/* The ergonomics eval (VAPP-108): how well an agent builds app screens
   from the catalog prompt. Every number comes from
   packages/exponential-ui/eval/report.json (score.ts, render.ts,
   parity.ts) and eval/rewrite-gaps.json; nothing is typed here by hand. */
import report from "@exponential-at/ui/eval/report.json"
import gaps from "@exponential-at/ui/eval/rewrite-gaps.json"
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable, PageHero } from "../components/Content"
import { componentHref } from "../lib/catalog-facts"
import { LEARN_NAV } from "../lib/content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `setup`, num: `01`, label: `The setup` },
  { id: `prompt`, num: `02`, label: `The prompt` },
  { id: `validator`, num: `03`, label: `What the validator said` },
  { id: `rubric`, num: `04`, label: `The visual rubric` },
  { id: `render`, num: `05`, label: `Rendered on the web` },
  { id: `parity`, num: `06`, label: `Spacing parity with the app` },
  { id: `vocabulary`, num: `07`, label: `What the corpus used` },
  { id: `gaps`, num: `08`, label: `The rewrite gap list` },
]

type Row = (typeof report.rows)[number]
type Measure = { width: number; height: number; nodes: number; gaps: number[]; distinctGaps: number[]; textInset: number | null; overflowsX: boolean }
type ParityRow = {
  platform: string
  native: { band: { height: number; titleX: number }; firstRow: { titleX: number; gapBelowBand: number } }
  template: { band: { height: number; titleX: number }; firstRow: { titleX: number; gapBelowBand: number } }
  delta: { bandHeight: number; bandTitleX: number; rowTitleX: number; gapBelowBand: number }
  bandDiff: { share: number }
}

const a = report.aggregate
const n = (x: number) => x.toLocaleString(`en-US`)
const pct = (x: number, d: number) => (d ? `${Math.round((x / d) * 100)} %` : `–`)
const render = report.render as Record<string, Measure[]>
const parity = (report.parity as { rows: ParityRow[] }).rows
const css = (storePx: number) => `${Math.round((storePx / 1.25) * 10) / 10} px`
const RUBRIC_LABEL: Record<string, string> = {
  primaryAction: `exactly one primary action`,
  emptyState: `an empty state where a list can be empty`,
  loading: `a loading state`,
  form: `every field inside a Form`,
  list: `a List or Table over a data template`,
  overlay: `the dialog, drawer or menu asked for`,
}
const CLASS_LABEL: Record<string, string> = {
  expressible: `Expressible today`,
  macro: `Needs a layout macro`,
  extension: `Needs an app extension component`,
  native: `Needs a new native`,
}

const findingsOf = (r: Row) => [...r.issues.map((i) => `${i.id}: ${i.message}`), ...r.findings]

export default function EvalPage({ path }: PageProps) {
  const source = (p: string) => LINKS.source(`packages/exponential-ui/eval/${p}`)
  return (
    <>
      <PageHero title="Ergonomics eval">
        <p>
          {a.surfaces} app-building prompts, answered from the catalog prompt, validated, rendered and scored. The
          numbers below are <a href={source(`report.json`)}>report.json</a>; CI replays the fixtures and never calls a
          model.
        </p>
      </PageHero>

      <DocsLayout nav={LEARN_NAV} title="Ergonomics eval" sections={SECTIONS} currentPath={path}>
        <DocsSection id="setup" num="01" label="The setup">
          <h2>The setup</h2>
          <p>
            <a href={source(`corpus.json`)}>The corpus</a> is what a product person types: a settings page, an issue
            list with filters, a device dashboard, a form with validation, a chat, a kanban, a data table, a wizard, an
            empty state, a phone inbox rendered at four widths. <code>record.ts</code> sends each prompt to Claude with{` `}
            <code>catalogPrompt()</code> as the system prompt and caches the reply as a fixture; <code>score.ts</code>{` `}
            validates every reply with the same reducer the renderers use and scores it; <code>render.ts</code> paints
            it in the React renderer and measures the boxes; <code>parity.ts</code> measures the Devices template
            against the native page on the stored shots.
          </p>
          <DocsCode>{`bun run --filter @exponential-at/ui eval:record   # once, needs ANTHROPIC_API_KEY
bun run --filter @exponential-at/ui eval          # score the fixtures → eval/report.json
bun run --filter @exponential-at/ui eval:render   # web renders at every width (Playwright)
bun run --filter @exponential-at/ui eval:parity   # the Devices template vs the native page`}</DocsCode>
          <p>
            Recorded by: {Object.entries(a.recordedBy).map(([k, v]) => `${v} ${k === `api` ? `over the API` : `by the session's own model from the same prompt`}`).join(`, `)}.
          </p>
        </DocsSection>

        <DocsSection id="prompt" num="02" label="The prompt">
          <h2>The prompt</h2>
          <DocsTable
            head={[`Variant`, `Components`, `Characters`, `Tokens (≈)`]}
            rows={[
              [<code key="v">catalogPrompt()</code>, report.prompt.full.components, n(report.prompt.full.chars), n(report.prompt.full.tokens)],
              [<code key="v">{`{ terse: true }`}</code>, report.prompt.terse.components, n(report.prompt.terse.chars), n(report.prompt.terse.tokens)],
              [<code key="v">{`{ lite: true }`}</code>, report.prompt.lite.components, n(report.prompt.lite.chars), n(report.prompt.lite.tokens)],
              [<code key="v">{`{ sections: ["rules", "index", "functions", "style"] }`}</code>, `${report.prompt.index.components} as one line each`, n(report.prompt.index.chars), n(report.prompt.index.tokens)],
              [<code key="v">{`{ sections: ["rules"] }`}</code>, `–`, n(report.prompt.rules.chars), n(report.prompt.rules.tokens)],
              [<code key="v">componentPrompt(name)</code>, `1`, `${report.prompt.perComponent.mean} on average, ${report.prompt.perComponent.max} at most`, `${report.prompt.perComponent.meanTokens} on average`],
            ]}
          />
          <p>
            The index prompt plus the {a.vocabulary.length} components the corpus actually used, served one block at a
            time, is the cheapest route to the same surfaces: about{` `}
            {n(report.prompt.index.tokens + a.vocabulary.length * report.prompt.perComponent.meanTokens)} tokens
            against {n(report.prompt.full.tokens)} for the full prompt.
          </p>
        </DocsSection>

        <DocsSection id="validator" num="03" label="What the validator said">
          <h2>What the validator said</h2>
          <DocsTable
            head={[`Metric`, `Value`]}
            rows={[
              [`Surfaces that validate clean`, `${a.validatorClean} of ${a.surfaces}`],
              [`Validator issues`, n(a.issuesTotal)],
              [`Components per surface`, `${a.components.mean} on average, ${a.components.max} at most`],
              [`Authored nesting depth`, `${a.depth.mean} on average, ${a.depth.max} at most`],
              [`Output tokens per surface`, `${n(a.tokens.meanOutput)}${a.tokens.estimated ? ` (estimated)` : ``}`],
            ]}
          />
          <h3>The mistakes, by frequency</h3>
          <DocsTable head={[`Issue`, `Count`]} rows={a.issueKinds.map((k) => [<code key={k.message}>{k.message}</code>, k.count])} />
        </DocsSection>

        <DocsSection id="rubric" num="04" label="The visual rubric">
          <h2>The visual rubric</h2>
          <DocsTable
            head={[`Criterion`, `Expected by`, `Passed`, `Rate`]}
            rows={Object.entries(a.rubric).map(([k, v]) => [RUBRIC_LABEL[k] ?? k, v.expected, v.passed, pct(v.passed, v.expected)])}
          />
          <DocsTable
            head={[`Habit`, `Count`]}
            rows={[
              [`Horizontal Stacks of controls without align (they stretch)`, a.unalignedRows],
              [`Authored Box components (the rule says: layout macros first)`, a.boxes],
              [`Surfaces with raw px spacing (rhythm ${a.spacing.rhythm})`, a.spacing.surfacesWithRawPx],
            ]}
          />
          <h3>Per surface</h3>
          <DocsTable
            head={[`Surface`, `Components`, `Depth`, `Issues`, `Findings`]}
            rows={report.rows.map((r) => [r.title, r.components, r.depth, r.issues.length, findingsOf(r).join(`; `) || `–`])}
          />
        </DocsSection>

        <DocsSection id="render" num="05" label="Rendered on the web">
          <h2>Rendered on the web</h2>
          <p>
            Every surface painted by the React renderer in the exponential theme at its widths; the gaps are the
            measured distances between the root&apos;s children, the inset is where the first text starts.
          </p>
          <DocsTable
            head={[`Surface`, `Width`, `Painted nodes`, `Height`, `Gaps between root children`, `First text inset`]}
            rows={report.rows.flatMap((r) =>
              (render[r.id] ?? []).map((m) => [r.title, `${m.width} px`, m.nodes, `${m.height} px`, m.distinctGaps.filter((g) => g >= 0).map((g) => `${g} px`).join(`, `) || `–`, m.textInset === null ? `–` : `${m.textInset} px`])
            )}
          />
        </DocsSection>

        <DocsSection id="parity" num="06" label="Spacing parity with the app">
          <h2>Spacing parity with the app</h2>
          <p>
            The Devices template (a declarative package rendered through the public host API) beside the native
            Devices page, measured on the stored shots of both: the Section band every list starts with, and the first
            row under it. Store pixels are 1.25 CSS px; the deltas are template minus native.
          </p>
          <DocsTable
            head={[`Platform`, `Band height`, `Band title inset`, `First row inset`, `Gap under the band`, `Band pixel diff`]}
            rows={parity.map((p) => [
              p.platform === `desktop` ? `Desktop (gpui)` : `Web`,
              `${css(p.native.band.height)} vs ${css(p.template.band.height)} (Δ ${css(p.delta.bandHeight)})`,
              `${css(p.native.band.titleX)} vs ${css(p.template.band.titleX)} (Δ ${css(p.delta.bandTitleX)})`,
              `${css(p.native.firstRow.titleX)} vs ${css(p.template.firstRow.titleX)} (Δ ${css(p.delta.rowTitleX)})`,
              `${css(p.native.firstRow.gapBelowBand)} vs ${css(p.template.firstRow.gapBelowBand)} (Δ ${css(p.delta.gapBelowBand)})`,
              `${(p.bandDiff.share * 100).toFixed(1)} %`,
            ])}
          />
        </DocsSection>

        <DocsSection id="vocabulary" num="07" label="What the corpus used">
          <h2>What the corpus used</h2>
          <DocsTable
            head={[`Component`, `Surfaces`, `Uses`]}
            rows={a.vocabulary.map((v) => [<a key={v.component} href={componentHref(v.component)}>{v.component}</a>, v.surfaces, v.uses])}
          />
          <p>
            Never used by the corpus ({a.unused.length}): {a.unused.map((c, i) => (
              <span key={c}>
                {i ? `, ` : ``}
                <a href={componentHref(c)}>{c}</a>
              </span>
            ))}.
          </p>
        </DocsSection>

        <DocsSection id="gaps" num="08" label="The rewrite gap list">
          <h2>The rewrite gap list</h2>
          <p>
            Every screen of the Exponential app, classified by what the catalog needs before it could be written as a
            surface. <a href={source(`rewrite-gaps.json`)}>The list</a> stays in lockstep with the view catalog.
          </p>
          <DocsTable
            head={[`Class`, `Screens`]}
            rows={gaps.classes.map((c) => [CLASS_LABEL[c] ?? c, gaps.screens.filter((s) => s.class === c).length])}
          />
          <h3>The candidate macros</h3>
          <DocsTable head={[`Macro`, `What it is`]} rows={Object.entries(gaps.candidates).map(([k, v]) => [<code key={k}>{k}</code>, v])} />
          <h3>Every screen</h3>
          <DocsTable head={[`Screen`, `Class`, `With`]} rows={gaps.screens.map((s) => [<code key={s.id}>{s.id}</code>, CLASS_LABEL[s.class] ?? s.class, s.with])} />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
