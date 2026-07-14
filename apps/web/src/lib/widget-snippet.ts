// Snippet builders shared by the workspace settings' widget section and the
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
    { mcpServers: { exponential: { url: `${origin}/api/mcp` } } },
    null,
    2
  )
}
