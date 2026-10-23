// VAPP-103: every href and every src the renderer emits passes the host
// contract's policy (`catalog/host.json` urls / media): Link (external or
// not), markdown links + images, FileUpload file urls, Image / Avatar /
// Video / Audio. A denied href paints as text; a denied src loads nothing.
// Relative urls resolve against the host's base (on the web, the
// document's when the host sets none). VAPP-107: a private, loopback or
// link-local host is denied unless the host's policy opts it in
// (`allowPrivate`); the page's OWN host is opted in when it is private (a
// dev server on localhost keeps its same-origin images and links), the LAN
// and every other private host stay denied. Shared by React and Solid
// (FEED-128): one filter, both renderers.

import { DEFAULT_MEDIA_SCHEMES, decideUrl, isPrivateHost, mediaRequest, safeHref, type MediaRequest, type UrlPolicy } from "@exponential_app/ui"
import type { AnyHostPlugin as HostPlugin } from "./host-types"

const documentBase = (): string | undefined => (typeof document !== `undefined` && document.baseURI ? document.baseURI : undefined)

/** VAPP-107: the page's OWN host when it is private (a dev server on
 *  localhost): the renderer opts it in on top of the host's `allowPrivate`,
 *  exactly as the Exponential app's hosts opt their own origin in. The page
 *  already runs there; nothing else private is reached. */
export function ownPrivateHosts(): string[] {
  try {
    const base = documentBase()
    if (!base) return []
    const host = new URL(base).hostname.toLowerCase()
    return isPrivateHost(host) ? [host] : []
  } catch {
    return []
  }
}

function withOwnOrigin(allow: readonly string[] | undefined): readonly string[] | undefined {
  const own = ownPrivateHosts()
  if (!own.length) return allow
  return [...(allow ?? []), ...own.filter((h) => !(allow ?? []).includes(h))]
}

/** The URL policy a plugin's hrefs pass. */
export function urlPolicyOf(host: HostPlugin): UrlPolicy {
  const urls = host.urls ?? {}
  const withBase = urls.baseUrl ? urls : { ...urls, baseUrl: host.media?.baseUrl ?? documentBase() }
  return { ...withBase, allowPrivate: withOwnOrigin(withBase.allowPrivate) }
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
  const req = host.mediaRequest ? host.mediaRequest(rewritten) : mediaRequest(rewritten, { baseUrl: documentBase(), ...host.media, allowPrivate: withOwnOrigin(host.media?.allowPrivate) })
  if (!req) return null
  return mediaUrlAllowed(host, req.url) ? req : null
}

/** Does an absolute media url pass the media schemes, hosts and private
 *  opt-ins (VAPP-107; the page's own private host counts as opted in)? */
export function mediaUrlAllowed(host: HostPlugin, url: string): boolean {
  return decideUrl({ schemes: host.media?.schemes ?? DEFAULT_MEDIA_SCHEMES, hosts: host.media?.hosts, allowPrivate: withOwnOrigin(host.media?.allowPrivate) }, url).allowed
}

/** The default opener: only what the policy allows, in a new tab. */
export function openAllowed(host: HostPlugin, url: string): void {
  const href = linkHref(host, url)
  if (!href) return
  if (host.openUrl) host.openUrl(href)
  else if (typeof window !== `undefined`) window.open(href, `_blank`, `noopener,noreferrer`)
}
