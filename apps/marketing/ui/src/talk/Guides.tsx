/* NEW-15: /talk/guides/, every Talk guide as a card (the /ui/ guides hub's twin). */
import { DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { CardGrid, PageHero } from "../components/Content"
import type { PageProps } from "../lib/routes"
import { TALK_DOCS_NAV, TALK_GUIDES, talkGuidePath } from "./nav"

const SECTIONS: DocsSectionType[] = [{ id: `guides`, num: `01`, label: `Guides` }]

export default function TalkGuides({ path }: PageProps) {
  return (
    <>
      <PageHero title="Guides" />
      <DocsLayout nav={TALK_DOCS_NAV} title="Talk" sections={SECTIONS} currentPath={path}>
        <DocsSection id="guides" num="01" label="Guides">
          <h2>Set it up</h2>
          <CardGrid items={TALK_GUIDES.map((g) => ({ href: talkGuidePath(g.slug), title: g.title, desc: g.blurb }))} />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
