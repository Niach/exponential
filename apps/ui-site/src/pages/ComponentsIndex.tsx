import { useEffect, useMemo, useRef, useState } from "react"
import { COMPONENT_DOCS, componentPath, componentSummary, groupedDocs, specimenById, THEMES_DOC, type ComponentDoc } from "../lib/catalog"
import type { PageProps } from "../lib/routes"
import { useScheme } from "../lib/scheme"
import { themeBackgrounds } from "../sdk/backgrounds"
import { caseAlign, studioTree, subjectOf, thumbProps } from "../sdk/ComponentStudio"
import { groupLabel, RichText } from "../sdk/labels"
import { MiniSurface } from "../sdk/MiniSurface"

/* The core catalog as a visual grid: every component a live mini render
   (mounted near the viewport), grouped, with search (`/` focuses it), a
   group filter (`?group=`, `?q=`). The cards are
   prerendered; only the renders wait for hydration. */

const GROUPS = groupedDocs()
const BACKGROUNDS = themeBackgrounds(THEMES_DOC)

/* Search words: the name (also split at its humps), the sentence, the group,
   A2UI basic aliases and prop names (split too). A query term matches a
   word's START, so `date` finds DatePicker but not validateOn. */
const words = (text: string) =>
  text
    .replace(/([a-z0-9])([A-Z])/g, `$1 $2`)
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean)
const searchWords = (doc: ComponentDoc) => new Set([doc.name.toLowerCase(), ...words([doc.name, doc.description, groupLabel(doc.group), ...doc.basic, ...doc.props.map((p) => p.name)].join(` `))])
const WORDS = new Map(COMPONENT_DOCS.map((d) => [d.name, [...searchWords(d)]]))

export const matches = (doc: ComponentDoc, query: string, group: string) => {
  if (group !== `all` && doc.group !== group) return false
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
  const list = WORDS.get(doc.name) ?? [...searchWords(doc)]
  return terms.every((t) => list.some((w) => w.startsWith(t)))
}

export default function ComponentsIndexPage(_: PageProps) {
  const [query, setQuery] = useState(``)
  const [group, setGroup] = useState(`all`)
  const search = useRef<HTMLInputElement>(null)
  const mode = useScheme()

  // ?q= and ?group= in, replaceState out (a filtered view is linkable).
  useEffect(() => {
    const params = new URLSearchParams(location.search)
    const g = params.get(`group`)
    if (g && GROUPS.some((x) => x.group === g)) setGroup(g)
    const q = params.get(`q`)
    if (q) setQuery(q)
  }, [])
  useEffect(() => {
    const params = new URLSearchParams()
    if (group !== `all`) params.set(`group`, group)
    if (query) params.set(`q`, query)
    const qs = params.toString()
    history.replaceState(null, ``, `${location.pathname}${qs ? `?${qs}` : ``}${location.hash}`)
  }, [query, group])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      if (e.key !== `/` || target?.closest(`input, textarea, select, [contenteditable]`)) return
      e.preventDefault()
      search.current?.focus()
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [])

  const visible = useMemo(() => new Set(COMPONENT_DOCS.filter((d) => matches(d, query, group)).map((d) => d.name)), [query, group])

  return (
    <div className="shell sdk-index">
      <header className="sdk-index-head">
        <h1>Components</h1>
        <div className="sdk-index-tools">
          <label className="sdk-search">
            <span className="sdk-sr-only">Search components</span>
            <input ref={search} type="search" className="sdk-input" placeholder="Search (press /)" value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <div className="sdk-filter" role="group" aria-label="Filter by group">
            <button type="button" className="sdk-chip" aria-pressed={group === `all`} onClick={() => setGroup(`all`)}>
              All
            </button>
            {GROUPS.map((g) => (
              <button key={g.group} type="button" className="sdk-chip" aria-pressed={group === g.group} onClick={() => setGroup(group === g.group ? `all` : g.group)}>
                {groupLabel(g.group)}
              </button>
            ))}
          </div>
        </div>
      </header>

      <p className="sdk-index-status" role="status">
        {visible.size === 0 ? `No component matches.` : ``}
      </p>
      <p className="sdk-sr-only" aria-live="polite">
        {visible.size === COMPONENT_DOCS.length || visible.size === 0 ? `` : `${visible.size} of ${COMPONENT_DOCS.length}`}
      </p>

      {GROUPS.map((g) => {
        const shown = g.docs.filter((d) => visible.has(d.name))
        return (
          <section key={g.group} id={g.group} className="sdk-index-group" hidden={shown.length === 0}>
            <h2>
              {groupLabel(g.group)}
            </h2>
            <div className="sdk-index-grid">
              {g.docs.map((doc) => (
                <ComponentCard key={doc.name} doc={doc} hidden={!visible.has(doc.name)} mode={mode} />
              ))}
            </div>
          </section>
        )
      })}
    </div>
  )
}

function ComponentCard({ doc, hidden, mode }: { doc: ComponentDoc; hidden: boolean; mode: `light` | `dark` }) {
  const tree = useMemo(() => {
    const specimen = specimenById(doc.specimenId)
    const subject = subjectOf(doc, specimen)
    return studioTree(doc, subject, thumbProps(doc, subject), caseAlign(specimen))
  }, [doc])
  return (
    <article className="sdk-card" hidden={hidden}>
      <div className="sdk-card-thumb">
        <MiniSurface domId={`card-${doc.specimenId}`} tree={tree} mode={mode} ground={BACKGROUNDS.exponential} zoom={0.72} minWidth={300} inert />
      </div>
      <div className="sdk-card-body">
        <h3 className="sdk-card-title">
          <a href={componentPath(doc)} className="sdk-card-link">
            {doc.name}
          </a>
        </h3>
        <p>
          <RichText text={componentSummary(doc)} />
        </p>
      </div>
    </article>
  )
}
