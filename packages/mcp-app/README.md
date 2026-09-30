# MCP App draft: the Exponential board view (EXP-1153)

A first draft of interactive UI for the Exponential MCP server, written
against **MCP Apps** (SEP-1865), the MCP extension that lets a tool return an
HTML view the host renders in a sandboxed iframe inside the conversation.
Nothing here is wired into the server yet; this folder is the drawing board.

| File | What it is |
| --- | --- |
| `board.html` | The View: the `ui://exponential/board` resource. Dependency-free, one file, host-themed. |
| `harness.html` | A fake host for local development: theme, tool-input/result, canned `tools/call`, a log. |

Run it:

```
cd packages/mcp-app && python3 -m http.server 8765
open http://localhost:8765/harness.html
```

## Is it a standard?

Yes. MCP Apps is an official extension of the Model Context Protocol
(`io.modelcontextprotocol/ui`, SEP-1865, spec at
github.com/modelcontextprotocol/ext-apps), co-developed by Anthropic and
OpenAI as the successor of OpenAI's Apps SDK. Hosts that render it today:
ChatGPT, Claude (web + desktop), VS Code Copilot, Microsoft 365 Copilot,
Goose, Postman, MCPJam. ChatGPT's own "extensions" (sidebar app, thread
panel, composer mentions, file viewers) sit ON TOP of it as `_meta["openai/ui"]`
additions; the view itself is portable.

## How it works

1. **Two MCP primitives.** The server registers a resource with a `ui://`
   URI and mime type `text/html;profile=mcp-app` whose body is the HTML, and
   a tool whose `_meta.ui.resourceUri` names that resource. Tools stay
   useful without the UI: `content` is what the model reads, `structuredContent`
   is what the view renders.
2. **The host renders it.** When the model calls the tool, the host fetches
   the resource with `resources/read` and puts the HTML in an iframe:
   `sandbox="allow-scripts"`, no cookies, no parent DOM, a CSP built from the
   resource's `_meta.ui.csp` (`connectDomains`, `resourceDomains`,
   `frameDomains`). Web hosts add a second, cross-origin proxy iframe.
3. **They talk JSON-RPC over `postMessage`.** The view is an MCP client, the
   host is its server. `ui/initialize` returns the host context (theme, CSS
   variables, display mode, locale, container size), then the host sends
   `ui/notifications/tool-input` and `ui/notifications/tool-result`. The
   view can call `tools/call` (proxied to the real server as the user, under
   the same OAuth grant), `ui/open-link`, `ui/message` (a follow-up typed as
   the user), `ui/update-model-context` (tell the model what is on screen),
   `ui/request-display-mode`, and reports `ui/notifications/size-changed`.
4. **Theming** is CSS custom properties the host hands over
   (`--color-background-primary`, `--font-sans`, `--border-radius-md` …).
   `board.html` reads them with our zinc dark as the fallback.

`board.html` implements the bridge by hand in forty lines; the official SDK
(`@modelcontextprotocol/ext-apps`, `App` class) does the same and adds typed
helpers. It requires MCP SDK 2.x peers, which the web app (SDK 1.x) is not on
yet, one reason the draft stays SDK-free.

## Server wiring (not applied yet)

```ts
// lib/mcp/tools.ts — next to the other registrations
server.registerResource(
  `exponential-board`,
  `ui://exponential/board`,
  {
    title: `Board view`,
    mimeType: `text/html;profile=mcp-app`,
    _meta: {
      ui: {
        // avatars and attachment thumbnails come from the app origin
        csp: { resourceDomains: [`https://app.exponential.at`] },
        prefersBorder: true,
      },
    },
  },
  async () => ({
    contents: [{ uri: `ui://exponential/board`, mimeType: `text/html;profile=mcp-app`, text: BOARD_HTML }],
  })
)

registerTool(
  `exponential_board_view`,
  {
    description: `Show a board's open issues grouped by status as an interactive view; the text result lists them too.`,
    inputSchema: strictInput({ boardId: uuidString }),
    _meta: {
      ui: { resourceUri: `ui://exponential/board`, visibility: [`model`, `app`] },
      // ChatGPT compatibility alias + its thread-panel entrypoint
      "openai/outputTemplate": `ui://exponential/board`,
      "openai/ui": { entrypoints: [{ type: `thread` }] },
    },
  },
  async ({ boardId }) => ({
    content: [{ type: `text`, text: summary }],   // for the model
    structuredContent: { board, issues },          // for the view
  })
)
```

`structuredContent` shape the view expects:

```
board:  { name, identifier, color?, url? }
issues: [{ identifier, title, url?, priority,
           status: { id, name, category, color },
           assignee?: { name, color? }, labels?: [{ name, color }],
           pr?: { state } }]
```

Add `exponential_board_view` to `lib/mcp/annotations.ts` (READ) and, for the
directory, re-scan: a plugin WITH UI must provide screenshots and a CSP
justification. `exponential_issues_get` already exists and is what the
detail panel calls.

## Trying it against a real host

- **Claude** (web/desktop): connect the MCP server; Claude renders MCP Apps.
- **ChatGPT**: developer mode (paid plans) or the listed plugin.
- **Local**: `harness.html` here, or the `basic-host` example in
  github.com/modelcontextprotocol/ext-apps, or MCPJam's inspector.

## What the production version changes

- Render with `@exp/ui` islands (`@exp/ui/island` already compiles the theme
  and shadcn set into a shadow root) with the host variables mapped onto the
  Tailwind tokens, bundled by Vite into one HTML file at build time.
- Live updates: no Electric in the iframe (no credentials there); the view
  re-runs the tool on a timer or on `ui/notifications/host-context-changed`,
  or the host pushes fresh results through `ui/notifications/tool-result`.
- A second view for a single issue (`ui://exponential/issue`) linked from
  `exponential_issues_get` and `exponential_issues_create`.
