/* The guides hub: every guide as a card, grouped by what you are doing. */
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { CardGrid, PageHero } from "../components/Content"
import { GUIDES_NAV } from "../lib/content"
import { GUIDES, guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const GROUPS: { id: string; label: string; title: string; intro: string; slugs: string[] }[] = [
  {
    id: `render`,
    label: `Render`,
    title: `Render A2UI on your platform`,
    intro: `Install a renderer, connect a host and paint the surface an agent sends. About twenty lines each.`,
    slugs: [`react`, `swiftui`, `compose`, `gpui`],
  },
  {
    id: `customise`,
    label: `Customise`,
    title: `Make it yours`,
    intro: `A theme changes how everything looks, an extension adds components, a host plugin decides what a surface may do.`,
    slugs: [`themes`, `extensions`, `host-plugins`],
  },
  {
    id: `build`,
    label: `Build`,
    title: `Build with agents`,
    intro: `Ship a whole app as a declarative package, or give a model the catalog and let it answer with UI.`,
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
      <PageHero title="Guides">
        From a blank project to a rendered surface on every platform, then themes, extensions, host plugins, vapps
        and agents. Every example on these pages is a real file that CI compiles from a fresh project.
      </PageHero>

      <DocsLayout nav={GUIDES_NAV} title="Guides" sections={SECTIONS} currentPath={path}>
        {GROUPS.map((group, i) => (
          <DocsSection key={group.id} id={group.id} num={SECTIONS[i].num} label={group.label}>
            <h2>{group.title}</h2>
            <p>{group.intro}</p>
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
            The examples live in <a href={LINKS.source(`apps/ui-site/guides`)}>apps/ui-site/guides</a>, and the
            pages show those files as they are. <code>guides/check.ts</code> copies each one into an empty temp
            directory and builds it the way you would: the npm packages packed and installed, the Swift package,
            the Maven artifacts from the local repository, the crates by path. Run one yourself:
          </p>
          <DocsCode>{`bun apps/ui-site/guides/check.ts react   # or agent, theme, swift, compose, gpui, all`}</DocsCode>
          <p>
            For complete apps, the <a href={LINKS.source(`samples/exponential-ui`)}>samples</a> run four hosts
            (web, iOS, Android, gpui) against one local A2UI server, in a third-party theme with a custom extension
            component.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
