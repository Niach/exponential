/* Every old exponential.at URL whose path changed (docs/site/audit.md § 4):
   301s, written into dist/serve.json at build. `:name` matches one path
   segment (serve-handler's syntax). Sources match with and without the
   trailing slash. */
export const REDIRECTS: Record<string, string> = {
  // Docs
  "/docs/getting-started": `/docs/get-started/`,
  "/docs/self-host": `/docs/get-started/self-host/`,
  "/docs/issues": `/docs/using/issues/`,
  "/docs/coding": `/docs/using/sessions/`,
  "/docs/feedback": `/docs/using/feedback/`,
  "/docs/widget": `/docs/using/widget/`,
  "/docs/apps": `/docs/eapps/`,
  "/docs/actions": `/docs/build/prompts/`,
  "/docs/cli": `/docs/build/cli/`,
  "/docs/mcp": `/docs/build/mcp/`,

  // The eUI section (/ui/ stays as eUI's product page)
  "/ui/concepts": `/docs/libraries/eui/`,
  "/ui/guides": `/docs/libraries/eui/guides/`,
  "/ui/guides/security": `/docs/libraries/eui/reference/security/`,
  "/ui/guides/apps": `/docs/eapps/build/`,
  "/ui/guides/motion": `/docs/libraries/motion/guides/animate/`,
  "/ui/guides/:slug": `/docs/libraries/eui/guides/:slug/`,
  "/ui/components": `/docs/libraries/eui/reference/components/`,
  "/ui/components/:slug": `/docs/libraries/eui/reference/components/:slug/`,
  "/ui/themes": `/docs/libraries/eui/reference/themes/`,
  "/ui/themes/builder": `/docs/libraries/eui/tools/theme-builder/`,
  "/ui/playground": `/docs/libraries/eui/tools/playground/`,
  "/ui/conformance": `/docs/libraries/eui/reference/conformance/`,
  "/ui/numbers": `/docs/libraries/eui/reference/numbers/`,
  "/ui/eval": `/docs/libraries/eui/reference/eval/`,

  // eTalk (/talk/ stays as its product page)
  "/talk/how-it-works": `/docs/libraries/etalk/`,
  "/talk/install": `/docs/libraries/etalk/guides/install/`,
  "/talk/guides": `/docs/libraries/etalk/guides/`,
  "/talk/guides/:slug": `/docs/libraries/etalk/guides/:slug/`,
  "/talk/api": `/docs/libraries/etalk/reference/api/`,
  "/talk/numbers": `/docs/libraries/etalk/reference/numbers/`,
  "/talk/licences": `/docs/libraries/etalk/reference/licences/`,

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
