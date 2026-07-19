/* Cross-page docs navigation — the single source of truth for the docs
   sidebar (DocsLayout), the hub card grid (DocsPage), and page ordering.
   Paths are load-bearing: the web app links straight to them, and
   scripts/prerender.tsx asserts this list stays in lockstep with the
   PAGES registry in seo.ts. */

export type DocsNavEntry = { path: string; label: string; blurb?: string }

export const DOCS_NAV: DocsNavEntry[] = [
  { path: `/docs/`, label: `Overview` },
  {
    path: `/docs/getting-started/`,
    label: `Getting started`,
    blurb: `Sign up, create your first board, connect GitHub, invite your team.`,
  },
  {
    path: `/docs/issues/`,
    label: `Issues & boards`,
    blurb: `The board, statuses, markdown, mentions, notifications, and how issues link to PRs.`,
  },
  {
    path: `/docs/coding/`,
    label: `Coding with Claude`,
    blurb: `Hand issues to Claude from the desktop IDE — single runs, batch runs, steer, review, merge.`,
  },
  {
    path: `/docs/feedback/`,
    label: `Feedback & helpdesk`,
    blurb: `The feedback widget, the team helpdesk, and the shared support inbox.`,
  },
  {
    path: `/docs/widget/`,
    label: `Feedback widget`,
    blurb: `Embed the feedback button on any site — snippet, JS API, screenshots.`,
  },
  {
    path: `/docs/mcp/`,
    label: `MCP & API`,
    blurb: `Connect Claude, ChatGPT, Cursor, or any MCP client to your issues.`,
  },
  {
    path: `/docs/apps/`,
    label: `Mobile & desktop apps`,
    blurb: `The desktop IDE and the iOS / Android companions — install, push, steer.`,
  },
  {
    path: `/docs/self-host/`,
    label: `Self-host`,
    blurb: `Run the whole stack on your own server with Docker Compose.`,
  },
]
