import { useEffect, useState } from "react"
import {
  IMAGE_ACTIONS_COPY,
  IMAGE_ACTIONS_ICONS,
  IMAGE_ACTIONS_TEST_IDS,
  copyEncoding,
  imageMenu,
  imageSource,
  shareFileName,
  type ImageInput,
  type ImageSource,
} from "@exp/domain-contract/image-actions"
import { IMAGE_NAME_ATTR, claimImageMenu, type ImageSourceVerdict } from "@exponential_app/eui/web"

import { conceptIcon } from "./icons.generated"
import { IMAGE_MENU_KIND, Menu, MenuGestureHost, type MenuEntry, type MenuSessionProps } from "./menu"
import { toast } from "./toast"

// #EXP-1926 / #EXP-1945 (packages/domain-contract/fixtures/image-actions.json,
// the web part): Copy image (and Share on a touch browser that can share a
// file) on every content picture, through THE web menu. A picture opts in by
// being one (`CONTENT_IMAGE_SELECTOR`: genUI's Image / ZoomableImage /
// Markdown images) or with `imageMenuProps(src)`; ONE `ImageMenuHost` per
// page answers a right-click or a touch long-press on it once its bytes
// rendered (`findMenuElement`'s image rule). The bytes go to the clipboard as
// PNG, never the url.

const CopyIcon = conceptIcon(IMAGE_ACTIONS_ICONS.copyImage as Parameters<typeof conceptIcon>[0])
const ShareIcon = conceptIcon(IMAGE_ACTIONS_ICONS.share as Parameters<typeof conceptIcon>[0])

const PNG = `image/png`

function origin(): string {
  return typeof window === `undefined` ? `http://localhost` : window.location.origin
}

/** The url a copy fetches: the src resolved against the page, the thumbnail
 *  width `w` dropped (the original, never a thumbnail); any other parameter
 *  stays. A `blob:` / `data:` src is used as it is. */
export function imageCopyUrl(src: string, base: string = origin()): string {
  let url: URL
  try {
    url = new URL(src, base)
  } catch {
    return src
  }
  if (url.protocol !== `http:` && url.protocol !== `https:`) return src
  if (url.searchParams.has(`w`)) {
    url.searchParams.delete(`w`)
    url.search = url.searchParams.toString()
  }
  return url.href
}

/** PNG bytes as they are; anything else decoded and re-encoded to PNG (an
 *  animation = its first frame, never downscaled). */
export async function toPngBlob(blob: Blob): Promise<Blob> {
  const head = new Uint8Array(await blob.slice(0, 8).arrayBuffer())
  if (copyEncoding(head) === `passthrough`) return blob.type === PNG ? blob : new Blob([blob], { type: PNG })
  const bitmap = await createImageBitmap(blob)
  try {
    const canvas = document.createElement(`canvas`)
    canvas.width = bitmap.width
    canvas.height = bitmap.height
    const context = canvas.getContext(`2d`)
    if (!context) throw new Error(`canvas 2d context unavailable`)
    context.drawImage(bitmap, 0, 0)
    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob((encoded) => (encoded ? resolve(encoded) : reject(new Error(`png encode failed`))), PNG)
    })
  } finally {
    bitmap.close()
  }
}

/** The picture's bytes as PNG, through the page's own authenticated loader
 *  (a same-origin fetch carries the session cookie). */
export async function imagePngBlob(src: string): Promise<Blob> {
  const response = await fetch(imageCopyUrl(src))
  if (!response.ok) throw new Error(`image fetch failed: ${response.status}`)
  return toPngBlob(await response.blob())
}

/** THE Copy image: call it INSIDE the gesture (a menu item's select). The
 *  clipboard write starts at once with a promise of the PNG, which keeps
 *  Safari's user activation; success = THE success toast, any failure (a
 *  failed load or decode, a browser without image clipboard) = THE error
 *  toast. Never a url fallback. */
export function copyImage(src: string): Promise<void> {
  const done = (async () => {
    if (typeof ClipboardItem === `undefined` || !navigator.clipboard?.write) throw new Error(`image clipboard unsupported`)
    await navigator.clipboard.write([new ClipboardItem({ [PNG]: imagePngBlob(src) })])
  })()
  return done.then(
    () => void toast.success(IMAGE_ACTIONS_COPY.copied),
    () => void toast.error(IMAGE_ACTIONS_COPY.copyFailed)
  )
}

/** Whether this browser can share a PNG FILE (`navigator.canShare`). */
export function canShareImageFile(): boolean {
  if (typeof navigator === `undefined` || typeof navigator.canShare !== `function` || typeof File === `undefined`) return false
  try {
    return navigator.canShare({ files: [new File([new Uint8Array(0)], `image.png`, { type: PNG })] })
  } catch {
    return false
  }
}

/** Share (touch only): the system share sheet with the PNG file (the same
 *  bytes as Copy), named `shareFileName(name)`; a cancelled sheet shows
 *  nothing, a failure THE error toast. */
export function shareImage(src: string, name?: string | null): Promise<void> {
  return (async () => {
    const png = await imagePngBlob(src)
    await navigator.share({ files: [new File([png], shareFileName(name), { type: PNG })] })
  })().catch((error: unknown) => {
    if (error instanceof DOMException && error.name === `AbortError`) return
    toast.error(IMAGE_ACTIONS_COPY.shareFailed)
  })
}

/** The image menu's rows for one input (`imageMenu`). */
export function imageMenuEntries(src: string, input: ImageInput, canShareFile: boolean, name?: string | null): MenuEntry[] {
  return imageMenu(input, canShareFile).map((item) =>
    item === `copyImage`
      ? { kind: `item`, id: IMAGE_ACTIONS_TEST_IDS.copyImage, "data-testid": IMAGE_ACTIONS_TEST_IDS.copyImage, label: IMAGE_ACTIONS_COPY.copyImage, icon: CopyIcon, onSelect: () => void copyImage(src) }
      : { kind: `item`, id: IMAGE_ACTIONS_TEST_IDS.share, "data-testid": IMAGE_ACTIONS_TEST_IDS.share, label: IMAGE_ACTIONS_COPY.share, icon: ShareIcon, onSelect: () => void shareImage(src, name) }
  )
}

function ImageMenu({ target, open, onOpenChange }: MenuSessionProps) {
  // Asked once per open: Share only where the browser shares a file.
  const [canShare] = useState(() => target.input === `touch` && canShareImageFile())
  const name = target.origin?.getAttribute(IMAGE_NAME_ATTR) ?? null
  return (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={open}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label={IMAGE_ACTIONS_COPY.copyImage}
      entries={imageMenuEntries(target.id, target.input, canShare, name)}
    />
  )
}

const IMAGE_MENUS = { [IMAGE_MENU_KIND]: (session: MenuSessionProps) => <ImageMenu {...session} /> }

/** THE image menu's host: mounted ONCE at the app's root. */
export function ImageMenuHost() {
  // Claimed while mounted: the elements around a picture leave it alone.
  useEffect(() => claimImageMenu(), [])
  return <MenuGestureHost menus={IMAGE_MENUS} />
}

/** `imageSource` as genUI's `host.imageSource`: the page's own origin. */
export function hostImageSource(src: string, alt: string): ImageSourceVerdict {
  return imageSource(src, alt, origin())
}

/** `imageSource` against the page's origin. */
export function pictureSource(src: string, alt = ``): ImageSource {
  return imageSource(src, alt, origin())
}

export { imageMenuProps } from "@exponential_app/eui/web"
