// Links into the docs of exponential.at (EXP-141; apps/site, Astro +
// Starlight). Pages use trailing slashes and live at their final URLs: link
// the destination, never an old path that apps/site/src/redirects.ts 301s
// (get-started/, get-started/self-host/, using/{issues,sessions,feedback,
// widget}/, eapps/, build/{prompts,cli,mcp}/).

export const DOCS_URL = `https://exponential.at/docs`

export function docsUrl(path = ``): string {
  return path ? `${DOCS_URL}/${path}/` : `${DOCS_URL}/`
}
