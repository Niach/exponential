/* Every old exponential.at URL whose path changed (docs/site/audit.md § 4):
   301s, written into dist/serve.json at build. `:name` matches one path
   segment (serve-handler's syntax). Sources match with and without the
   trailing slash. */
/** A moved docs tree: its root and every page down to four segments below it. */
function moved(from: string, to: string): Record<string, string> {
  const rules: Record<string, string> = { [from]: `${to}/` }
  for (const path of [`:a`, `:a/:b`, `:a/:b/:c`, `:a/:b/:c/:d`]) rules[`${from}/${path}`] = `${to}/${path}/`
  return rules
}

export const REDIRECTS: Record<string, string> = {
  // Docs
  "/docs/getting-started": `/docs/get-started/`,
  "/docs/self-host": `/docs/get-started/self-host/`,
  "/docs/issues": `/docs/using/issues/`,
  "/docs/coding": `/docs/using/sessions/`,
  "/docs/feedback": `/docs/using/feedback/`,
  "/docs/widget": `/docs/using/widget/`,
  "/docs/apps": `/docs/sdk/apps/`,
  "/docs/actions": `/docs/build/prompts/`,
  "/docs/cli": `/docs/build/cli/`,
  "/docs/mcp": `/docs/build/mcp/`,

  // The product pages moved under /sdk/ (old name → module)
  "/ui": `/sdk/genui/`,
  "/motion": `/sdk/motion/`,
  "/talk": `/sdk/voice/`,
  "/net": `/sdk/p2p/`,
  "/pilot": `/sdk/computer-use/`,
  "/mcp": `/sdk/mcp-proxy/`,
  "/agent": `/sdk/agents/`,
  "/sdk/acp": `/sdk/agents/`,
  "/core": `/sdk/core/`,
  "/apps": `/sdk/apps/`,
  "/sdk": `/docs/sdk/`,

  // The docs of each module moved from /docs/libraries/ and /docs/eapps/
  "/docs/libraries": `/docs/sdk/`,
  "/docs/libraries/install": `/docs/sdk/install/`,
  ...moved(`/docs/libraries/eui`, `/docs/sdk/genui`),
  ...moved(`/docs/libraries/motion`, `/docs/sdk/motion`),
  ...moved(`/docs/libraries/etalk`, `/docs/sdk/voice`),
  ...moved(`/docs/libraries/enet`, `/docs/sdk/p2p`),
  ...moved(`/docs/libraries/epilot`, `/docs/sdk/computer-use`),
  ...moved(`/docs/libraries/emcp`, `/docs/sdk/mcp-proxy`),
  ...moved(`/docs/libraries/eagent`, `/docs/sdk/agents`),
  ...moved(`/docs/sdk/acp`, `/docs/sdk/agents`),
  ...moved(`/docs/libraries/ecore`, `/docs/sdk/core`),
  ...moved(`/docs/eapps`, `/docs/sdk/apps`),
  "/docs/reference/spec/eui-round-:n": `/docs/reference/spec/genui-round-:n/`,

  // The old /ui/ section
  "/ui/concepts": `/docs/sdk/genui/`,
  "/ui/guides": `/docs/sdk/genui/guides/`,
  "/ui/guides/security": `/docs/sdk/genui/reference/security/`,
  "/ui/guides/apps": `/docs/sdk/apps/build/`,
  "/ui/guides/motion": `/docs/sdk/motion/guides/animate/`,
  "/ui/guides/:slug": `/docs/sdk/genui/guides/:slug/`,
  "/ui/components": `/docs/sdk/genui/reference/components/`,
  "/ui/components/:slug": `/docs/sdk/genui/reference/components/:slug/`,
  "/ui/themes": `/docs/sdk/genui/reference/themes/`,
  "/ui/themes/builder": `/docs/sdk/genui/tools/theme-builder/`,
  "/ui/playground": `/docs/sdk/genui/tools/playground/`,
  "/ui/conformance": `/docs/sdk/genui/reference/conformance/`,
  "/ui/numbers": `/docs/sdk/genui/reference/numbers/`,
  "/ui/eval": `/docs/sdk/genui/reference/eval/`,

  // The old /talk/ section
  "/talk/how-it-works": `/docs/sdk/voice/`,
  "/talk/install": `/docs/sdk/voice/guides/install/`,
  "/talk/guides": `/docs/sdk/voice/guides/`,
  "/talk/guides/:slug": `/docs/sdk/voice/guides/:slug/`,
  "/talk/api": `/docs/sdk/voice/reference/api/`,
  "/talk/numbers": `/docs/sdk/voice/reference/numbers/`,
  "/talk/licences": `/docs/sdk/voice/reference/licences/`,

  // Astro's sitemap integration writes sitemap-index.xml
  "/sitemap.xml": `/sitemap-index.xml`,

  // Carried over from apps/marketing/public/serve.json
  "/blog/what-an-assistant-should-do": `/blog/ui-as-data-down-to-the-compositor/`,
  "/skills/voice-reference-clip/SKILL.md": `/talk/skills/voice-reference-clip/SKILL.md`,
}

/** The destination for a request path, or null (exact rules win over patterns). */
export function redirectFor(path: string): string | null {
  const bare = path.length > 1 ? path.replace(/\/$/, ``) : path
  if (REDIRECTS[bare]) return REDIRECTS[bare]!
  for (const [source, destination] of Object.entries(REDIRECTS)) {
    if (!source.includes(`:`)) continue
    const names: string[] = []
    const re = new RegExp(`^${source.replace(/:(\w+)/g, (_, n: string) => (names.push(n), `([^/]+)`))}$`)
    const m = re.exec(bare)
    if (m) return names.reduce((to, n, i) => to.replace(`:${n}`, m[i + 1]!), destination)
  }
  return null
}
