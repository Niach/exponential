import { DocsLayout, DocsSection } from "@exp/site-shell"
import { COMPONENT_DOCS, SHOT_PLATFORMS, THEMES_DOC, componentBySlug, componentPath, specimenById, type ComponentDoc } from "../lib/catalog"
import type { PageProps } from "../lib/routes"
import { appParityFor, type AppParity, type ParityCell, type ParityPlatform } from "../data/app-parity"
import { themeBackgrounds } from "../sdk/backgrounds"
import { ComponentStudio } from "../sdk/ComponentStudio"
import { groupLabel, KindBadges } from "../sdk/labels"
import { ShotFigure } from "../sdk/Shot"

/* One page per core component (the slug from the path): the model-facing
   description, the live studio (render + props switcher + A2UI JSON), the
   four platform shots, the props table, slots/events/aliases, the theme's
   recipe parts and where the Exponential app uses it. The component's
   "dialog": everything about it on one page, all from generated data. */

const REPO_BLOB = `https://github.com/Niach/exponential/blob/master/`
const PLATFORM_ORDER: { id: ParityPlatform; label: string }[] = [
  { id: `web`, label: `Web` },
  { id: `ios`, label: `iOS` },
  { id: `android`, label: `Android` },
  { id: `desktop`, label: `Desktop` },
]

export default function ComponentPage({ path }: PageProps) {
  const slug = path.split(`/`).filter(Boolean)[1] ?? ``
  const doc = componentBySlug(slug)
  if (!doc) return <div className="shell docs-hero">Unknown component.</div>
  const index = COMPONENT_DOCS.indexOf(doc)
  const prev = COMPONENT_DOCS[index - 1]
  const next = COMPONENT_DOCS[index + 1]
  const specimen = specimenById(doc.specimenId)
  const recipe = THEMES_DOC.components.find((c) => c.name === doc.name)
  const parity = appParityFor(doc.name)

  const sections = [
    { id: `live`, num: `01`, label: `Live render` },
    { id: `platforms`, num: `02`, label: `Every platform` },
    { id: `props`, num: `03`, label: `Props` },
    { id: `contract`, num: `04`, label: `Slots, events, theming` },
    ...(parity.length ? [{ id: `in-the-app`, num: `05`, label: `In the Exponential app` }] : []),
  ]

  return (
    <div className="sdk-page-component">
      <DocsLayout nav={COMPONENT_DOCS.map((d) => ({ path: componentPath(d), label: d.name }))} title="Components" sections={sections} currentPath={path}>
        <header className="sdk-component-head">
          <a className="sdk-crumb" href="/components/">
            Components · {groupLabel(doc.group)}
          </a>
          <h1>
            {doc.name} <KindBadges doc={doc} />
          </h1>
          <p className="sdk-lead">{doc.description}</p>
          {doc.macro && <p className="sdk-note">{doc.macro}</p>}
        </header>

        <DocsSection id="live" num="01" label="Live render">
          <p>
            Rendered right here by <code>@exponential-at/ui-react</code>. Switch the props below; the render and the JSON follow.
          </p>
          <ComponentStudio doc={doc} specimen={specimen} themes={THEMES_DOC.themes} backgrounds={themeBackgrounds(THEMES_DOC)} />
        </DocsSection>

        <DocsSection id="platforms" num="02" label="Every platform">
          <p>The same specimen surface painted natively by each renderer (the stored shots of view <code>{doc.specimenId}</code>).</p>
          <div className="sdk-shots">
            {SHOT_PLATFORMS.map((p) => (
              <ShotFigure key={p.id} viewId={doc.specimenId} platform={p} title={doc.name} />
            ))}
          </div>
        </DocsSection>

        <DocsSection id="props" num="03" label="Props">
          <PropsTable doc={doc} />
        </DocsSection>

        <DocsSection id="contract" num="04" label="Slots, events, theming">
          <dl className="sdk-facts">
            <dt>Kind</dt>
            <dd>{doc.kind === `macro` ? `Macro: expands into natives before painting, so every renderer gets it for free.` : `Native: every renderer paints it.`}</dd>
            <dt>Children</dt>
            <dd>
              <code>{doc.children}</code>
            </dd>
            <dt>Slots</dt>
            <dd>{doc.slots.length ? doc.slots.map((s) => <code key={s}>{s}</code>) : `none`}</dd>
            <dt>Events</dt>
            <dd>{doc.events.length ? doc.events.map((e) => <code key={e}>on.{e}</code>) : `none`}</dd>
            <dt>A2UI basic</dt>
            <dd>{doc.basic.length ? doc.basic.map((b) => <code key={b}>{b}</code>) : `no basic-catalog alias`}</dd>
            <dt>Core lite</dt>
            <dd>{doc.lite ? `yes` : `no (left out of the lite catalog)`}</dd>
            {recipe && (
              <>
                <dt>Recipe parts</dt>
                <dd>
                  {recipe.parts.map((p) => (
                    <code key={p}>
                      {doc.name}/{p}
                    </code>
                  ))}
                </dd>
                <dt>Recipe props</dt>
                <dd>{recipe.props.length ? recipe.props.map((p) => <code key={p}>{p}</code>) : `none (state only)`}</dd>
              </>
            )}
          </dl>
          <p className="sdk-note">
            A theme styles each part with rules keyed by <code>when</code> (the recipe props and the states {THEMES_DOC.states.join(`, `)}). See{` `}
            <a href="/themes/">Themes</a> and <a href="/guides/themes/">Write your own theme</a>.
          </p>
        </DocsSection>

        {parity.length > 0 && (
          <DocsSection id="in-the-app" num="05" label="In the Exponential app">
            <p>Where the Exponential app draws the same thing with its own components on each platform.</p>
            {parity.map((row) => (
              <ParityRow key={row.id} row={row} />
            ))}
          </DocsSection>
        )}

        <nav className="sdk-prevnext" aria-label="Components">
          {prev ? (
            <a href={componentPath(prev)}>
              <span>Previous</span>
              <strong>{prev.name}</strong>
            </a>
          ) : (
            <span />
          )}
          {next ? (
            <a href={componentPath(next)} className="is-next">
              <span>Next</span>
              <strong>{next.name}</strong>
            </a>
          ) : (
            <span />
          )}
        </nav>
      </DocsLayout>
    </div>
  )
}

function PropsTable({ doc }: { doc: ComponentDoc }) {
  if (doc.props.length === 0) return <p>No props.</p>
  return (
    <div className="sdk-table-wrap">
      <table className="sdk-table">
        <thead>
          <tr>
            <th>Prop</th>
            <th>Type</th>
            <th>Default</th>
            <th>Description</th>
          </tr>
        </thead>
        <tbody>
          {doc.props.map((p) => {
            const def = (p as { default?: unknown }).default
            return (
              <tr key={p.name}>
                <td>
                  <code className="sdk-prop-name">{p.name}</code>
                  {p.required && <span className="sdk-req">required</span>}
                  {p.bindable && (
                    <span className="sdk-bindable" title="Accepts a data binding {path}">
                      bindable
                    </span>
                  )}
                </td>
                <td>
                  <code className="sdk-type">{String(p.type)}</code>
                </td>
                <td>{def === undefined ? <span className="sdk-dim">–</span> : <code>{JSON.stringify(def)}</code>}</td>
                <td>{p.description}</td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

const STATUS_LABEL: Record<string, string> = { ok: `ok`, leftover: `leftover`, "n/a": `n/a` }

function ParityRow({ row }: { row: AppParity }) {
  return (
    <div className="sdk-parity">
      <h3>{row.title}</h3>
      <p>{row.blurb}</p>
      <div className="sdk-parity-grid">
        {PLATFORM_ORDER.map((p) => (
          <ParityCellView key={p.id} label={p.label} cell={row.status[p.id]} />
        ))}
      </div>
      {row.leftovers && row.leftovers.length > 0 && (
        <ul className="sdk-parity-leftovers">
          {row.leftovers.map((l) => (
            <li key={`${l.file}-${l.note}`}>
              <a href={`${REPO_BLOB}${l.file}`}>{l.file}</a>: {l.note}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function ParityCellView({ label, cell }: { label: string; cell: ParityCell | undefined }) {
  const status = cell?.status ?? `n/a`
  return (
    <div className={`sdk-parity-cell is-${status === `n/a` ? `na` : status}`}>
      <div className="sdk-parity-head">
        <strong>{label}</strong>
        <span className="sdk-parity-status">{STATUS_LABEL[status] ?? status}</span>
      </div>
      {cell?.symbol && <code>{cell.symbol}</code>}
      {cell?.file && (
        <a href={`${REPO_BLOB}${cell.file}`} className="sdk-parity-file">
          {cell.file}
        </a>
      )}
      {cell?.note && <p>{cell.note}</p>}
    </div>
  )
}
