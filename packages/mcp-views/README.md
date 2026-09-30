# @exp/mcp-views — the Exponential MCP App views (EXP-1153)

Interactive UI for the Exponential MCP server, written to **MCP Apps**
(SEP-1865), the MCP extension that lets a tool return an HTML view the host
renders in a sandboxed iframe inside the conversation. ChatGPT, Claude (web
and desktop), VS Code Copilot, Goose and Postman render it; ChatGPT's own
extensions (thread panel, sidebar, composer mentions) are `_meta["openai/ui"]`
additions on top, and the view itself is portable.

| View | Resource | Tool | What it shows |
| --- | --- | --- | --- |
| `board` | `ui://exponential/board` | `exponential_board_view` | A board's open issues grouped by status, with a detail panel. Also the ChatGPT thread-panel entrypoint. |
| `issue` | `ui://exponential/issue` | `exponential_issue_view` | One issue: status, assignee, labels, description, comments, relations, PR. |

## How it works

1. **Two MCP primitives.** The web server registers each view as a resource
   with a `ui://` URI and mime type `text/html;profile=mcp-app`
   (`apps/web/src/lib/mcp/views.ts`), and a tool whose `_meta.ui.resourceUri`
   names it. The tool's text result is for the model; its `structuredContent`
   (typed in `src/contract.ts`) is what the view renders. A host without MCP
   Apps sees the text alone and loses nothing.
2. **The host renders it.** When the model calls the tool, the host reads
   the resource and injects the HTML into an iframe: `sandbox="allow-scripts"`,
   no cookies, no parent DOM, a CSP that allows no external origin (the views
   need none: avatars render as initials).
3. **They talk JSON-RPC over `postMessage`** (`src/bridge.ts`, forty lines,
   no SDK). `ui/initialize` returns the host context; the host pushes
   `ui/notifications/tool-input` and `tool-result`; the view calls
   `tools/call` (proxied to our server as the user, under the same OAuth
   grant: the board's detail panel calls `exponential_issue_view`),
   `ui/open-link`, `ui/message` (a follow-up typed as the user),
   `ui/update-model-context` (what the user is looking at) and reports
   `ui/notifications/size-changed`.
4. **Theming.** The host hands over CSS variables (`--color-background-primary`,
   `--font-sans`, …); `src/shell.tsx` maps them onto the web theme's tokens
   (`--background`, `--border`, …) as inline styles on `<html>`, so the host
   wins where it speaks and our zinc dark fills the rest. `theme: "light"`
   drops the `dark` class.

## Build

```
bun run build:mcp-views        # → packages/mcp-views/dist/{board,issue}.html
```

`scripts/build.ts` runs one Vite IIFE pass per view, compiles the `@exp/ui`
stylesheet once with the island compiler, and inlines both into a single
HTML document. The web server reads `dist/` at request time; Docker builds
it before the web app. Views import `@exp/ui` files directly
(`@exp/ui/src/button`), never the barrel: the barrel drags highlight.js,
the zod-backed domain and sonner into every bundle.

## Local development

```
cd packages/mcp-views && python3 -m http.server 8765
open http://localhost:8765/harness/?view=board          # or ?view=issue, &theme=light
```

`harness/index.html` plays the host with sample data and logs every message
the view sends. Against a real host: connect the MCP server in Claude or
ChatGPT and ask for a board.

## Adding a view

1. Add its name to `VIEW_NAMES` and its data type to `src/contract.ts`.
2. `src/<name>.tsx` mounts it with `mountView`; the component gets
   `{ data, refresh, bridge }`.
3. A tool in `apps/web/src/lib/mcp/tools.ts` with
   `_meta: viewToolMeta("<name>")` returning `viewResult(text, data)`, and a
   row in `lib/mcp/annotations.ts`.
