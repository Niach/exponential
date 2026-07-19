// Snippet builders shared by the team settings' widget section and the
// "Getting started" cards (EXP-88). Origin is a parameter (callers pass
// `window.location.origin`) so the builders stay SSR-safe and unit-testable;
// deriving from the current origin keeps every snippet self-host correct.

export function buildWidgetSnippet(publicKey: string, origin: string): string {
  const scriptUrl = `${origin}/widget/v1/loader.js`
  return `<script>
  (function (w, d, u) {
    if (w.ExponentialWidget) return;
    var q = [], api = { q: q };
    ["init","identify","setCustomData","open","close"].forEach(function (m) {
      api[m] = function () { q.push([m, [].slice.call(arguments)]); };
    });
    w.ExponentialWidget = api;
    var s = d.createElement("script");
    s.async = true; s.src = u;
    d.head.appendChild(s);
  })(window, document, "${scriptUrl}");
  ExponentialWidget.init({ key: "${publicKey}" });
  // Optional: attach your signed-in user for the helpdesk flow.
  // ExponentialWidget.identify({ email: "user@example.com", name: "Jane" });
  // ExponentialWidget.setCustomData({ plan: "pro" });
</script>`
}

// The `mcpServers` config from the public docs (marketing DocsPage §07),
// pointed at this instance's /api/mcp.
export function buildMcpServersConfig(origin: string): string {
  return JSON.stringify(
    { mcpServers: { exponential: { url: buildMcpEndpoint(origin) } } },
    null,
    2
  )
}

// Per-client MCP setup snippets (EXP-141 — the getting-started MCP tabs).
// Same contract as the builders above: origin is a parameter so everything
// stays SSR-safe, unit-testable, and self-host correct.

export function buildMcpEndpoint(origin: string): string {
  return `${origin}/api/mcp`
}

// Claude Code: one command registers the server for the user, `/mcp` inside
// claude then signs in via OAuth.
export function buildMcpAddCommand(origin: string): string {
  return `claude mcp add --transport http --scope user exponential ${buildMcpEndpoint(origin)}`
}

// Codex CLI: register, then the explicit OAuth login step.
export function buildCodexMcpAddCommand(origin: string): string {
  return `codex mcp add exponential --url ${buildMcpEndpoint(origin)}
codex mcp login exponential`
}

// Stdio-only clients bridge to the HTTP endpoint via mcp-remote.
export function buildMcpRemoteBridgeCommand(origin: string): string {
  return `npx mcp-remote ${buildMcpEndpoint(origin)}`
}
