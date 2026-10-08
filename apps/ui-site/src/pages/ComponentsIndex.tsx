import { COMPONENT_DOCS, COMPONENT_GROUPS, componentPath, type ComponentDoc } from "../lib/catalog"
import type { PageProps } from "../lib/routes"
import { groupLabel, KindBadges } from "../sdk/labels"
import { ShotImage } from "../sdk/Shot"

/* The core catalog, grouped: one card per component (its web shot, the
   model-facing sentence, native/macro + lite badges), each linking to its
   page. Everything here is prerendered markup; the data is the generated
   docs, so new components appear without a change here. */
export default function ComponentsIndexPage(_: PageProps) {
  const groups = COMPONENT_GROUPS.map((group) => ({ group, docs: COMPONENT_DOCS.filter((d) => d.group === group) })).filter((g) => g.docs.length > 0)
  const ungrouped = COMPONENT_DOCS.filter((d) => !COMPONENT_GROUPS.includes(d.group))
  if (ungrouped.length) groups.push({ group: `other`, docs: ungrouped })
  const natives = COMPONENT_DOCS.filter((d) => d.kind === `native`).length
  return (
    <div className="shell sdk-index">
      <header className="docs-hero">
        <div className="docs-hero-content">
          <span className="section-eyebrow">Core catalog</span>
          <h1>Components</h1>
          <p>
            {COMPONENT_DOCS.length} components an agent can put on a surface: {natives} natives every renderer paints, {COMPONENT_DOCS.length - natives} macros that expand into
            natives before painting. Each page renders it live in React, shows the A2UI JSON and its shots from SwiftUI, Compose and gpui.
          </p>
        </div>
        <nav className="sdk-index-jump" aria-label="Groups">
          {groups.map((g) => (
            <a key={g.group} href={`#${g.group}`} className="sdk-chip">
              {groupLabel(g.group)} <span>{g.docs.length}</span>
            </a>
          ))}
        </nav>
      </header>
      {groups.map((g) => (
        <section key={g.group} id={g.group} className="sdk-index-group">
          <h2>
            {groupLabel(g.group)} <span className="sdk-count">{g.docs.length}</span>
          </h2>
          <div className="sdk-index-grid">
            {g.docs.map((doc) => (
              <ComponentCard key={doc.name} doc={doc} />
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}

function ComponentCard({ doc }: { doc: ComponentDoc }) {
  return (
    <a className="sdk-card" href={componentPath(doc)}>
      <div className="sdk-card-thumb">
        <ShotImage viewId={doc.specimenId} platform="web" alt={`${doc.name} rendered by the React renderer`} />
      </div>
      <div className="sdk-card-body">
        <div className="sdk-card-title">
          <span>{doc.name}</span>
          <KindBadges doc={doc} />
        </div>
        <p>{doc.description}</p>
      </div>
    </a>
  )
}
