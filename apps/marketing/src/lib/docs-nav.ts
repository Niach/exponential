/* Cross-page docs navigation — the single source of truth for the docs
   sidebar (DocsLayout), the hub card grid (DocsPage), and page ordering.
   Paths are load-bearing: the web app links straight to them. */

export const DOCS_NAV: { path: string; label: string }[] = [
  { path: `/docs/`, label: `Overview` },
  { path: `/docs/getting-started/`, label: `Getting started` },
  { path: `/docs/issues/`, label: `Issues & boards` },
  { path: `/docs/coding/`, label: `Coding with Claude` },
  { path: `/docs/feedback/`, label: `Feedback & helpdesk` },
  { path: `/docs/widget/`, label: `Feedback widget` },
  { path: `/docs/mcp/`, label: `MCP & API` },
  { path: `/docs/apps/`, label: `Mobile & desktop apps` },
  { path: `/docs/self-host/`, label: `Self-host` },
]
