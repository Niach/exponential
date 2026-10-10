// VAPP-103: every href and every src the renderer emits passes the host
// contract's policy (`catalog/host.json` urls / media): Link (external or
// not), markdown links + images, FileUpload file urls, Image / Avatar /
// Video / Audio. A denied href paints as text; a denied src loads nothing.
// Relative urls resolve against the host's base (on the web, the
// document's when the host sets none).

import { DEFAULT_MEDIA_SCHEMES, decideUrl, mediaRequest, safeHref, type MediaRequest, type UrlPolicy } from "@exponential-at/ui"
import type { HostPlugin } from "./host"

const documentBase = (): string | undefined => (typeof document !== `undefined` && document.baseURI ? document.baseURI : undefined)

/** The URL policy a plugin's hrefs pass. */
export function urlPolicyOf(host: HostPlugin): UrlPolicy {
  const urls = host.urls ?? {}
  return urls.baseUrl ? urls : { ...urls, baseUrl: host.media?.baseUrl ?? documentBase() }
}

/** The href a link may navigate to, or undefined (paint it as text). */
export function linkHref(host: HostPlugin, href: unknown): string | undefined {
  return safeHref(urlPolicyOf(host), href)
}

/** The media request for a src (`resolveUrl` rewrites first; the host's
 *  `mediaRequest` or the plugin's `media` options build it), re-checked
 *  against the media schemes/hosts so a host hook cannot widen them. Null =
 *  nothing loads. */
export function mediaRequestOf(host: HostPlugin, src: unknown): MediaRequest | null {
  if (typeof src !== `string` || !src) return null
  const rewritten = host.resolveUrl ? host.resolveUrl(src) : src
  const req = host.mediaRequest ? host.mediaRequest(rewritten) : mediaRequest(rewritten, { baseUrl: documentBase(), ...host.media })
  if (!req) return null
  return mediaUrlAllowed(host, req.url) ? req : null
}

/** Does an absolute media url pass the media schemes/hosts? */
export function mediaUrlAllowed(host: HostPlugin, url: string): boolean {
  return decideUrl({ schemes: host.media?.schemes ?? DEFAULT_MEDIA_SCHEMES, hosts: host.media?.hosts }, url).allowed
}

/** The default opener: only what the policy allows, in a new tab. */
export function openAllowed(host: HostPlugin, url: string): void {
  const href = linkHref(host, url)
  if (!href) return
  if (host.openUrl) host.openUrl(href)
  else if (typeof window !== `undefined`) window.open(href, `_blank`, `noopener,noreferrer`)
}
