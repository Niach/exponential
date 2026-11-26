// genUI round 4: a Box's `backgroundImage` on the web. The node sheet paints
// the layers (theme-css `backgroundDeclarations`: gradient, scrim, then
// `var(--xui-bgi)`); the renderer loads the picture the way an Image does
// (the host's image-source rule, the media request) and sets `--xui-bgi` on
// the element: the placeholder fill until the picture DECODED, then its url.

import { recipeStyle, type ResolvedTheme } from "@exponential_app/eui"
import { BACKGROUND_IMAGE_VAR, backgroundPicture, backgroundPlaceholder, tokenVar } from "./theme-css"

/** The node's picture src (`style.backgroundImage.src`), else ``. */
export function backgroundImageSrc(style: Record<string, unknown> | undefined): string {
  const b = style?.backgroundImage
  if (typeof b !== `object` || b === null) return ``
  const src = (b as { src?: unknown }).src
  return typeof src === `string` ? src : ``
}

/** The placeholder fill: the theme's `Image/fallback` fill, else muted. */
export function backgroundPlaceholderFill(theme: ResolvedTheme | undefined): string | null {
  const bg = theme ? recipeStyle(theme, { component: `Image`, part: `fallback` }).backgroundColor : undefined
  return typeof bg === `string` ? (bg.startsWith(`$`) ? tokenVar(bg) : bg) : null
}

/** The `--xui-bgi` declaration for a picture: its url once `decoded`,
 *  else the placeholder. */
export function backgroundImageVar(url: string | undefined, decoded: boolean, theme: ResolvedTheme | undefined): Record<string, string> {
  return { [BACKGROUND_IMAGE_VAR]: url && decoded ? backgroundPicture(url) : backgroundPlaceholder(backgroundPlaceholderFill(theme)) }
}

/** Resolves once the browser decoded `url` (rejects on an error); without
 *  a DOM image it never settles, so the placeholder stays. */
export function decodePicture(url: string): Promise<void> {
  return new Promise((resolve, reject) => {
    if (typeof Image === `undefined`) return
    const img = new Image()
    img.onload = () => resolve()
    img.onerror = () => reject(new Error(`picture failed`))
    img.src = url
  })
}
