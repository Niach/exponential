/* The guides hub: every guide as a card, grouped by what you are doing. */
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { CardGrid, PageHero } from "../components/Content"
import { GUIDES_NAV } from "../lib/content"
import { GUIDES, guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const GROUPS: { id: string; label: string; title: string; slugs: string[] }[] = [
  {
    id: `render`,
    label: `Render`,
    title: `Render A2UI on your platform`,
    slugs: [`react`, `swiftui`, `compose`, `gpui`],
  },
  {
    id: `customise`,
    label: `Customise`,
    title: `Make it yours`,
    slugs: [`themes`, `extensions`, `host-plugins`],
  },
  {
    id: `build`,
    label: `Build`,
    title: `Build with agents`,
    slugs: [`vapps`, `agents`],
  },
]

const SECTIONS: DocsSectionType[] = [
  ...GROUPS.map((g, i) => ({ id: g.id, num: String(i + 1).padStart(2, `0`), label: g.label })),
  { id: `ci`, num: String(GROUPS.length + 1).padStart(2, `0`), label: `Compiled in CI` },
]

export default function GuidesPage({ path }: PageProps) {
  return (
    <>
      <PageHero title="Guides" />

      <DocsLayout nav={GUIDES_NAV} title="Guides" sections={SECTIONS} currentPath={path}>
        {GROUPS.map((group, i) => (
          <DocsSection key={group.id} id={group.id} num={SECTIONS[i].num} label={group.label}>
            <h2>{group.title}</h2>
            <CardGrid
              items={group.slugs.map((slug) => {
                const guide = GUIDES.find((g) => g.slug === slug)!
                return { href: guidePath(slug), title: guide.title, desc: guide.blurb }
              })}
            />
          </DocsSection>
        ))}

        <DocsSection id="ci" num={SECTIONS[SECTIONS.length - 1].num} label="Compiled in CI">
          <h2>Compiled in CI</h2>
          <p>
            The pages show the files in <a href={LINKS.source(`apps/ui-site/guides`)}>apps/ui-site/guides</a> as they are. <code>guides/check.ts</code> builds each in an empty temp directory from the packed npm packages, the Swift package, local Maven artifacts and path crates:
          </p>
          <DocsCode>{`bun apps/ui-site/guides/check.ts react   # or agent, theme, swift, compose, gpui, all`}</DocsCode>
          <p>
            Complete apps: the <a href={LINKS.source(`samples/exponential-ui`)}>samples</a> run web, iOS, Android and gpui hosts against one local A2UI server, with a third-party theme and an extension component.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
