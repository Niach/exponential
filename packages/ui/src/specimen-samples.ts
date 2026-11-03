import { useSyncExternalStore } from "react"
import samples from "@exponential_app/eui/samples/samples.json"

// EXP-1654: the committed sample pictures a SPECIMEN paints instead of a
// placeholder (packages/exponential-ui/samples, read samples.json's
// `$comment`): the eUI kitchen sink + specimens resolve an example src
// (`specimenSampleUrl`, a host's `resolveUrl`), the plan-page and story-tab
// specimens an attachment id (`specimenAttachmentSrc`). Vite turns each
// `new URL(…, import.meta.url)` into an emitted asset (dev: /@fs/), so a
// bundle carries a url, never the bytes. The product never reads it. SSR
// sees a file: url (Vite rewrites these for the client only), so a page the
// server renders asks `useSpecimenAttachmentSrc`: the transparent pixel
// while hydrating, the sample right after.

const FILES: Record<string, string> = {
  "alex.png": new URL(`../../exponential-ui/samples/alex.png`, import.meta.url).href,
  "cover.png": new URL(`../../exponential-ui/samples/cover.png`, import.meta.url).href,
  "data.png": new URL(`../../exponential-ui/samples/data.png`, import.meta.url).href,
  "editor.png": new URL(`../../exponential-ui/samples/editor.png`, import.meta.url).href,
  "ios-picker.png": new URL(`../../exponential-ui/samples/ios-picker.png`, import.meta.url).href,
  "one.png": new URL(`../../exponential-ui/samples/one.png`, import.meta.url).href,
  "preview.png": new URL(`../../exponential-ui/samples/preview.png`, import.meta.url).href,
  "screenshot.png": new URL(`../../exponential-ui/samples/screenshot.png`, import.meta.url).href,
  "two.png": new URL(`../../exponential-ui/samples/two.png`, import.meta.url).href,
  "versions.png": new URL(`../../exponential-ui/samples/versions.png`, import.meta.url).href,
  "web-picker.png": new URL(`../../exponential-ui/samples/web-picker.png`, import.meta.url).href,
}

const URLS = samples.urls as Record<string, string>
const ATTACHMENTS = samples.attachments as Record<string, string>

/** An example src samples.json `urls` names → its sample's url, else the src. */
export function specimenSampleUrl(src: string): string {
  const file = URLS[src]
  return (file && FILES[file]) || src
}

/** An attachment id → its sample's url (samples.json `attachments`), else
 *  the product's `/api/attachments/<id>`. */
export function specimenAttachmentSrc(attachmentId: string): string {
  const file = ATTACHMENTS[attachmentId]
  return (file && FILES[file]) || `/api/attachments/${attachmentId}`
}

/** A transparent pixel: the tile's muted fill shows through. */
const PIXEL = `data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7`
const noop = () => () => {}

/** `specimenAttachmentSrc` once on the client (hydration-safe), else the
 *  transparent pixel. */
export function useSpecimenAttachmentSrc(): (attachmentId: string) => string {
  const client = useSyncExternalStore(
    noop,
    () => true,
    () => false
  )
  return client ? specimenAttachmentSrc : () => PIXEL
}
