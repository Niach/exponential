import { DocsLayout, DocsSection } from "@exp/site-shell"
import {
  ORDERED_DOCS,
  SHOT_PLATFORMS,
  THEMES_DOC,
  componentBySlug,
  componentPath,
  componentSummary,
  keyboardRows,
  macroParts,
  relatedComponents,
  specimenById,
  componentByName,
  type ComponentDoc,
} from "../lib/catalog"
import type { PageProps } from "../lib/routes"
import { appParityFor, type AppParity, type ParityCell, type ParityPlatform } from "../data/app-parity"
import { themeBackgrounds } from "../sdk/backgrounds"
import { ComponentStudio } from "../sdk/ComponentStudio"
import { VariantGallery, hasVariants } from "../sdk/VariantGallery"
import { groupLabel, KindBadges, RichText } from "../sdk/labels"
import { ShotFigure } from "../sdk/Shot"

/* One page per core component (the slug from the path), all from generated
   data: the live studio (render, switches, props, JSON, embed code), the four
   platform shots, the variants, props, events and slots, keyboard, theming
   parts, where the Exponential app uses it, related components. */

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
  const index = ORDERED_DOCS.indexOf(doc)
  const prev = ORDERED_DOCS[index - 1]
  const next = ORDERED_DOCS[index + 1]
  const specimen = specimenById(doc.specimenId)
  const recipe = THEMES_DOC.components.find((c) => c.name === doc.name)
  const parity = appParityFor(doc.name)
  const backgrounds = themeBackgrounds(THEMES_DOC)
  const variants = hasVariants(doc, specimen)
  const keys = keyboardRows(doc)
  const parts = doc.kind === `macro` ? macroParts(doc.name) : []
  const related = relatedComponents(doc)

  const sections = [
    { id: `preview`, label: `Preview` },
    { id: `platforms`, label: `Platforms` },
    ...(variants ? [{ id: `variants`, label: `Variants` }] : []),
    { id: `props`, label: `Props` },
    { id: `events-slots`, label: `Events and slots` },
    { id: `accessibility`, label: `Accessibility` },
    { id: `theming`, label: `Theming` },
    ...(parity.length ? [{ id: `in-the-app`, label: `In the Exponential app` }] : []),
  ].map((s, i) => ({ ...s, num: String(i + 1).padStart(2, `0`) }))
  const num = (id: string) => sections.find((s) => s.id === id)!.num

  return (
    <div className="sdk-page-component">
      <DocsLayout nav={ORDERED_DOCS.map((d) => ({ path: componentPath(d), label: d.name, group: groupLabel(d.group) }))} title="Components" sections={sections} currentPath={path}>
        <header className="sdk-component-head">
          <a className="sdk-crumb" href={`/components/?group=${doc.group}`}>
            Components / {groupLabel(doc.group)}
          </a>
          <h1>
            {doc.name} <KindBadges doc={doc} />
          </h1>
          <p className="sdk-lead">
            <RichText text={componentSummary(doc)} />
          </p>
          <dl className="sdk-meta">
            {parts.length > 0 && (
              <div>
                <dt>Built from</dt>
                <dd>
                  {parts.map((p) => (
                    <a key={p} className="sdk-chip" href={componentPath(componentByName(p)!)}>
                      {p}
                    </a>
                  ))}
                </dd>
              </div>
            )}
            {doc.basic.length > 0 && (
              <div>
                <dt>A2UI basic</dt>
                <dd>
                  {doc.basic.map((b) => (
                    <code key={b}>{b}</code>
                  ))}
                </dd>
              </div>
            )}
            <div>
              <dt>Children</dt>
              <dd>
                <code>{doc.children}</code>
              </dd>
            </div>
          </dl>
        </header>

        <DocsSection heading id="preview" num={num(`preview`)} label="Preview">
          <ComponentStudio doc={doc} specimen={specimen} themes={THEMES_DOC.themes} backgrounds={backgrounds} />
        </DocsSection>

        <DocsSection heading id="platforms" num={num(`platforms`)} label="Platforms">
          <div className="sdk-shots">
            {SHOT_PLATFORMS.map((p) => (
              <ShotFigure key={p.id} viewId={doc.specimenId} platform={p} title={doc.name} />
            ))}
          </div>
          <p className="sdk-note">
            View <code>{doc.specimenId}</code>, painted by each native renderer.
          </p>
        </DocsSection>

        {variants && (
          <DocsSection heading id="variants" num={num(`variants`)} label="Variants">
            <VariantGallery doc={doc} specimen={specimen} backgrounds={backgrounds} />
          </DocsSection>
        )}

        <DocsSection heading id="props" num={num(`props`)} label="Props">
          <PropsTable doc={doc} />
        </DocsSection>

        <DocsSection heading id="events-slots" num={num(`events-slots`)} label="Events and slots">
          <EventsSlots doc={doc} />
        </DocsSection>

        <DocsSection heading id="accessibility" num={num(`accessibility`)} label="Accessibility">
          <dl className="sdk-facts">
            <dt>Role</dt>
            <dd>
              <code>{doc.accessibility?.role ?? `none`}</code>
            </dd>
            {doc.accessibility?.notes && (
              <>
                <dt>Semantics</dt>
                <dd>
                  <RichText text={doc.accessibility.notes} />
                </dd>
              </>
            )}
          </dl>
          {keys.length > 0 ? (
            <div className="sdk-table-wrap">
              <table className="sdk-table sdk-keys">
                <thead>
                  <tr>
                    <th scope="col">Keys</th>
                    <th scope="col">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {keys.map((k, i) => (
                    <tr key={i}>
                      <td>{k.keys.length ? k.keys.map((key) => <kbd key={key}>{key}</kbd>) : <span className="sdk-dim">–</span>}</td>
                      <td>
                        <RichText text={k.action} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="sdk-note">Not focusable; no keys.</p>
          )}
        </DocsSection>

        <DocsSection heading id="theming" num={num(`theming`)} label="Theming">
          {recipe ? (
            <dl className="sdk-facts">
              <dt>Recipe parts</dt>
              <dd>
                {recipe.parts.map((p) => (
                  <code key={p}>
                    {doc.name}/{p}
                  </code>
                ))}
              </dd>
              <dt>Keyed by</dt>
              <dd>
                {recipe.props.map((p) => (
                  <code key={p}>{p}</code>
                ))}
                {THEMES_DOC.states.map((s) => (
                  <code key={s} className="sdk-dim">
                    :{s}
                  </code>
                ))}
              </dd>
            </dl>
          ) : (
            <p className="sdk-note">No recipe: styled through the components it is built from.</p>
          )}
          <p className="sdk-note">
            <a href="/themes/">Themes</a> · <a href="/guides/themes/">Write a theme</a>
          </p>
        </DocsSection>

        {parity.length > 0 && (
          <DocsSection heading id="in-the-app" num={num(`in-the-app`)} label="In the Exponential app">
            {parity.map((row) => (
              <ParityRow key={row.id} row={row} />
            ))}
          </DocsSection>
        )}

        {related.length > 0 && (
          <nav className="sdk-related" aria-label="Related components">
            <h2>Related</h2>
            <div>
              {related.map((r) => (
                <a key={r.name} className="sdk-chip" href={componentPath(r)}>
                  {r.name}
                </a>
              ))}
            </div>
          </nav>
        )}

        <nav className="sdk-prevnext" aria-label="Previous and next component">
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

/** The first sentence of a prop description (the table keeps one line). */
const firstSentence = (text: string) => {
  const m = text.match(/^(.+?[.!?])(\s|$)/)
  return m ? m[1]! : text
}

function PropsTable({ doc }: { doc: ComponentDoc }) {
  if (doc.props.length === 0) return <p className="sdk-note">No props.</p>
  return (
    <div className="sdk-table-wrap">
      <table className="sdk-table sdk-props">
        <thead>
          <tr>
            <th scope="col">Prop</th>
            <th scope="col">Type</th>
            <th scope="col">Default</th>
            <th scope="col">Description</th>
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
                      bind
                    </span>
                  )}
                  {p.responsive && (
                    <span className="sdk-bindable is-responsive" title="Takes a per-breakpoint object">
                      resp
                    </span>
                  )}
                </td>
                <td>
                  <code className="sdk-type">{String(p.type)}</code>
                </td>
                <td>{def === undefined ? <span className="sdk-dim">–</span> : <code>{JSON.stringify(def)}</code>}</td>
                <td>
                  <RichText text={firstSentence(p.description)} />
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

function EventsSlots({ doc }: { doc: ComponentDoc }) {
  const rows = [...doc.events.map((e) => ({ kind: `event`, name: `on.${e}` })), ...doc.slots.map((s) => ({ kind: `slot`, name: s }))]
  if (rows.length === 0) return <p className="sdk-note">No events, no slots. Children: <code>{doc.children}</code>.</p>
  return (
    <div className="sdk-table-wrap">
      <table className="sdk-table">
        <thead>
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Kind</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={`${r.kind}-${r.name}`}>
              <td>
                <code className="sdk-prop-name">{r.name}</code>
              </td>
              <td>{r.kind === `event` ? `event: runs an action` : `slot: one component by id`}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

const STATUS_LABEL: Record<string, string> = { ok: `ok`, leftover: `leftover`, "n/a": `n/a` }

function ParityRow({ row }: { row: AppParity }) {
  return (
    <div className="sdk-parity">
      <h3 className="sdk-parity-title">{row.title}</h3>
      <ParityBlurb text={row.blurb} />
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

/** The first sentence; the rest folds (the parity notes run long). */
function ParityBlurb({ text }: { text: string }) {
  const head = firstSentence(text)
  const rest = text.slice(head.length).trim()
  return (
    <div className="sdk-parity-blurb">
      <p>{head}</p>
      {rest && (
        <details>
          <summary>More</summary>
          <p>{rest}</p>
        </details>
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
