// The one exponential.at (spike, docs/site/audit.md): Astro pages for the
// landing pages, Starlight for the docs and the product pages, one header.
import { defineConfig } from "astro/config"
import react from "@astrojs/react"
import starlight from "@astrojs/starlight"
import starlightLlmsTxt from "starlight-llms-txt"
import { euiStyles } from "./src/integrations/eui-styles.ts"
import { serveConfig } from "./src/integrations/serve-config.ts"
import { shotStore } from "./src/integrations/shot-store.ts"
import { SIDEBAR } from "./src/sidebar/index.ts"

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
      customCss: [`./src/styles/fonts.css`, `./src/styles/theme.css`, `./src/styles/landing.css`, `./src/styles/parts.css`, `./src/styles/docs.css`, `./src/styles/libraries.css`],
      components: {
        Header: `./src/components/Header.astro`,
        Head: `./src/components/Head.astro`,
        Footer: `./src/components/Footer.astro`,
        ThemeProvider: `./src/components/ThemeProvider.astro`,
        ThemeSelect: `./src/components/ThemeSelect.astro`,
      },
      social: [{ icon: `github`, label: `GitHub`, href: `https://github.com/Niach/exponential` }],
      editLink: { baseUrl: `https://github.com/Niach/exponential/edit/master/apps/site/` },
      lastUpdated: false,
      pagefind: true,
      plugins: [
        starlightLlmsTxt({
          projectName: `Exponential`,
          description: `Exponential is a real-time issue tracker with coding agents, and the Exponential SDK it is built on, with the modules genUI (generative native UI), Motion, Voice, P2P, Computer Use, MCP Proxy, Agents, Core and Apps.`,
        }),
      ],
      sidebar: SIDEBAR,
      routeMiddleware: `./src/route-data.ts`,
    }),
    react(),
    euiStyles(),
    serveConfig(),
  ],
})
