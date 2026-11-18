// #EXP-1945 (Exponential's fixtures/image-actions.json, the web part): what
// the React and Solid renderers share for a content picture's menu and its
// source. Framework-free.
//
// THE MENU: a renderer draws no menu of its own for a picture. Its CONTENT
// pictures (Image, the slides of Carousel / Mosaic = Image nodes,
// ZoomableImage, a Markdown block image) are found by
// `CONTENT_IMAGE_SELECTOR`, and the HOST's one document-level menu answers a
// right-click / long-press on one whose bytes rendered (`readyContentImage`).
// Avatars, video posters, the Carousel's indicator thumbnails and the choice
// tiles are not content pictures. A host's own pictures opt in with
// `imageMenuProps(src)`.
//
// THE SOURCE: `host.imageSource(src, alt)` narrows which pictures load at
// all (Exponential's `imageSource`): `load` = through the media policy as
// before, `link` = an ordinary link in the picture's place (THE link look,
// the URL policy and the host's opener), `text` = the alt as plain text.
// Unset = every src is `load` (the media policy alone decides).

import type { AnyHostPlugin } from "./host-types"

/** A host's own `<img>` that answers the image menu; its value is the src to
 *  copy (empty = the element's own `currentSrc`). */
export const IMAGE_MENU_ATTR = `data-image-menu`

/** Every content picture a web renderer (or a host) draws. */
export const CONTENT_IMAGE_SELECTOR = [
  `img[${IMAGE_MENU_ATTR}]`,
  `[data-xui-c="Image"] > img`,
  `[data-xui-part="Markdown/image"] > img`,
  `img[data-xui-part="ZoomableImage/image"]`,
  `img[data-slot="zoomable-image-image"]`,
  `[data-slot="mosaic-tile"] > img`,
].join(`, `)

/** A host picture's file name (a shared file's name), when it knows one. */
export const IMAGE_NAME_ATTR = `data-image-name`

/** The attributes a host's own picture spreads to answer the image menu. */
export function imageMenuProps(src?: string, name?: string | null): Record<string, string> {
  return name ? { [IMAGE_MENU_ATTR]: src ?? ``, [IMAGE_NAME_ATTR]: name } : { [IMAGE_MENU_ATTR]: src ?? `` }
}

/** The content picture an event landed ON (the `<img>` itself), else null. */
export function contentImageAt(target: EventTarget | null): HTMLImageElement | null {
  if (typeof HTMLImageElement === `undefined` || !(target instanceof HTMLImageElement)) return null
  return target.matches(CONTENT_IMAGE_SELECTOR) ? target : null
}

/** Its bytes rendered (not loading, failed or empty). */
export function imageReady(img: HTMLImageElement): boolean {
  return img.complete && img.naturalWidth > 0
}

/** The content picture under the event when it is READY: the image menu
 *  answers it; anything else falls through to the element's own menu. */
export function readyContentImage(target: EventTarget | null): HTMLImageElement | null {
  const img = contentImageAt(target)
  return img && imageReady(img) ? img : null
}

/** The src a copy fetches: the host's (`imageMenuProps`), else the one on
 *  screen (a renderer's fetched blob url or the direct url). */
export function contentImageSrc(img: HTMLImageElement): string {
  return img.getAttribute(IMAGE_MENU_ATTR) || img.currentSrc || img.src
}

/** What a picture's src renders as (see the header). */
export type ImageSourceVerdict = { kind: `load` } | { kind: `link`; href: string; text: string } | { kind: `text`; text: string }

const LOAD: ImageSourceVerdict = { kind: `load` }

/** The host's verdict for one picture, `load` without the hook. */
export function imageSourceOf(host: AnyHostPlugin, src: string, alt: string): ImageSourceVerdict {
  if (!src || !host.imageSource) return LOAD
  return host.imageSource(src, alt) ?? LOAD
}

// THE CLAIM: a page whose host answers the image menu claims it (a counter:
// one per mounted host). While claimed, a renderer's own context Menu (and
// any host element with a menu of its own) leaves a press on a ready
// content picture to it, never merged; unclaimed, nothing changes.
let claims = 0

/** The host's image menu is mounted; call the returned release on unmount. */
export function claimImageMenu(): () => void {
  claims += 1
  let released = false
  return () => {
    if (released) return
    released = true
    claims -= 1
  }
}

/** The press is the image menu's (claimed, on a ready content picture):
 *  the element around it opens nothing. */
export function imageMenuOwns(target: EventTarget | null): boolean {
  return claims > 0 && readyContentImage(target) !== null
}
