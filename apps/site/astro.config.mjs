// The one exponential.at (spike, docs/site/audit.md): Astro pages for the
// landing pages, Starlight for the docs and the product pages, one header.
import { defineConfig } from "astro/config"
import react from "@astrojs/react"
import starlight from "@astrojs/starlight"
import starlightLlmsTxt from "starlight-llms-txt"
import { euiStyles } from "./src/integrations/eui-styles.ts"
import { serveConfig } from "./src/integrations/serve-config.ts"
import { shotStore } from "./src/integrations/shot-store.ts"
import { LIBRARIES } from "./src/lib/libraries.ts"

/* Libraries: each one Overview / Guides / Reference (stubs until phase 3,
   Motion's reference is real: docs/spec/motion.md). */
const libraryGroups = LIBRARIES.map((lib) => ({
  label: lib.name,
  collapsed: lib.slug !== `motion` && lib.slug !== `eui`,
  items: [
    { label: `Overview`, link: `/docs/libraries/${lib.slug}/` },
    { label: `Guides`, link: `/docs/libraries/${lib.slug}/guides/` },
    { label: `Reference`, link: `/docs/libraries/${lib.slug}/reference/` },
  ],
}))

export default defineConfig({
  site: `https://exponential.at`,
  trailingSlash: `always`,
  integrations: [
    shotStore(),
    starlight({
      title: `Exponential`,
      description: `The issue tracker where coding agents work in sessions.`,
      logo: { dark: `./public/logo-light.svg`, light: `./public/logo-dark.svg` },
      favicon: `/favicon.ico`,
      customCss: [`./src/styles/fonts.css`, `./src/styles/theme.css`],
      components: {
        Header: `./src/components/Header.astro`,
        Head: `./src/components/Head.astro`,
      },
      social: [{ icon: `github`, label: `GitHub`, href: `https://github.com/Niach/exponential` }],
      editLink: { baseUrl: `https://github.com/Niach/exponential/edit/master/apps/site/` },
      lastUpdated: false,
      pagefind: true,
      plugins: [
        starlightLlmsTxt({
          projectName: `Exponential`,
          description: `Exponential is a real-time issue tracker with coding agents, and the open libraries it is built on: eUI (generative native UI), Motion, eTalk, eNet, ePilot, eMCP, eAgent, eCore and eApps.`,
        }),
      ],
      sidebar: [
        {
          label: `Get started`,
          items: [
            { label: `Overview`, link: `/docs/` },
            { label: `Quick start`, link: `/docs/get-started/` },
            { label: `Self-host`, link: `/docs/get-started/self-host/` },
          ],
        },
        {
          label: `Using Exponential`,
          items: [
            { label: `Issues and boards`, link: `/docs/using/issues/` },
            { label: `Sessions`, link: `/docs/using/sessions/` },
            { label: `Feedback`, link: `/docs/using/feedback/` },
            { label: `Widget`, link: `/docs/using/widget/` },
            { label: `Mobile apps`, link: `/docs/using/mobile-apps/` },
          ],
        },
        {
          label: `eApps`,
          items: [
            { label: `What eApps are`, link: `/docs/eapps/` },
            { label: `Build an eApp`, link: `/docs/eapps/build/` },
            { label: `Package reference`, link: `/docs/eapps/reference/` },
          ],
        },
        {
          label: `Build on Exponential`,
          items: [
            { label: `MCP tools`, link: `/docs/build/mcp/` },
            { label: `CLI`, link: `/docs/build/cli/` },
            { label: `Prompts`, link: `/docs/build/prompts/` },
          ],
        },
        { label: `Libraries`, items: libraryGroups },
        {
          label: `Reference`,
          items: [
            { label: `Changelog`, link: `/docs/reference/changelog/` },
            { label: `Spec index`, link: `/docs/reference/spec/` },
            { label: `llms.txt`, link: `/llms.txt`, attrs: { target: `_blank` } },
          ],
        },
      ],
    }),
    react(),
    euiStyles(),
    serveConfig(),
  ],
})
