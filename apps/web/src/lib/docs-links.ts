// Links into the marketing site's docs (EXP-141). The docs are a separate
// deploy (exponential.at, apps/marketing) with one page per topic — pages use
// trailing slashes (getting-started/, coding/, feedback/, widget/, mcp/,
// apps/, issues/, self-host/).

export const DOCS_URL = `https://exponential.at/docs`

export function docsUrl(path = ``): string {
  return path ? `${DOCS_URL}/${path}/` : `${DOCS_URL}/`
}
