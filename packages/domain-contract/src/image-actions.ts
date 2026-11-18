import fixture from "../fixtures/image-actions.json" with { type: "json" }

// #EXP-1926 / #EXP-1944 (fixtures/image-actions.json): Copy image on every
// content picture, and which picture sources load at all. TS reference; the
// platform parts (web, desktop + eDE, iOS, Android, the genUI renderers)
// replay `menuCases`, `pressCases`, `encodingCases`, `shareNameCases` and
// `sourceCases`; the copy reaches Swift/Kotlin/Rust via DomainContract.generated.

export const IMAGE_ACTIONS_COPY = fixture.copy
export const IMAGE_ACTIONS_ICONS = fixture.icons
export const IMAGE_ACTIONS_TEST_IDS = fixture.testIds
export const IMAGE_ACTIONS_TOASTS = fixture.toasts
export const IMAGE_ACTIONS_VIEWS = fixture.views
export const IMAGE_COPY_FORMAT = fixture.copyFormat
export const IMAGE_SOURCE_POLICY = fixture.sourcePolicy

export type ImageInput = `pointer` | `touch`
export type ImageMenuItem = `copyImage` | `share`

/** The image menu's items in order: pointer = Copy image, touch = Copy image
 *  then Share (only when the platform can share a file). */
export function imageMenu(input: ImageInput, canShareFile: boolean): ImageMenuItem[] {
  const items = fixture.menus[input] as ImageMenuItem[]
  return items.filter((item) => item !== `share` || canShareFile)
}

/** Which menu a press opens: the image's own on a rendered picture, else the
 *  element's around it, else none. Never merged. */
export function pressMenu(onImage: boolean, imageReady: boolean, ownerMenu: string | null): `image` | `owner` | `none` {
  if (onImage && imageReady) return `image`
  return ownerMenu ? `owner` : `none`
}

const PNG_SIGNATURE = fixture.copyFormat.pngSignature.match(/../g)!.map((h) => Number.parseInt(h, 16))

/** PNG bytes go to the clipboard as they are; anything else is decoded and
 *  re-encoded to PNG (an animation = its first frame). */
export function copyEncoding(head: Uint8Array): `passthrough` | `reencode` {
  if (head.length < PNG_SIGNATURE.length) return `reencode`
  return PNG_SIGNATURE.every((b, i) => head[i] === b) ? `passthrough` : `reencode`
}

/** The shared PNG's file name: the last extension becomes `.png`, path
 *  separators become `-`, an empty stem (or no name) = `image.png`. */
export function shareFileName(name: string | null | undefined): string {
  const clean = (name ?? ``).trim().replace(/[/\\]/g, `-`)
  const dot = clean.lastIndexOf(`.`)
  const stem = (dot > 0 ? clean.slice(0, dot) : clean).trim()
  return `${stem || `image`}.png`
}

export type ImageSource =
  | { kind: `load` }
  | { kind: `link`; href: string; text: string }
  | { kind: `text`; text: string }

const UUID = `[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}`
// The path exactly `/api/attachments/<uuid>`, then an optional query and/or
// fragment (no `%`, no further `/` before them).
const ATTACHMENT_PATH = new RegExp(`^${IMAGE_SOURCE_POLICY.attachmentPath}${UUID}(?:\\?[^#]*)?(?:#.*)?$`)
// Whitespace or control anywhere (after the trim) = not a url.
const NOT_URL = /[\u0000- \u007f]/
// Surrounding ASCII whitespace (space, tab, LF, FF, CR).
const EDGE_SPACE = /^[ \t\n\f\r]+|[ \t\n\f\r]+$/g

function isAttachmentPath(path: string): boolean {
  return ATTACHMENT_PATH.test(path)
}

function isLoadableData(lower: string): boolean {
  const comma = lower.indexOf(`,`)
  if (comma < 0) return false
  const mime = lower.slice(5, comma).split(`;`)[0]!
  const prefix = IMAGE_SOURCE_POLICY.dataMimePrefix
  return mime.startsWith(prefix) && mime.length > prefix.length && !IMAGE_SOURCE_POLICY.deniedDataMimes.includes(mime)
}

/** The host of a `scheme://` or `//` url for the link label: lowercased,
 *  userinfo and port dropped, IPv6 brackets kept; empty = no host. */
function linkHost(afterSlashes: string): string {
  const end = afterSlashes.search(/[/?#\\]/)
  let authority = end < 0 ? afterSlashes : afterSlashes.slice(0, end)
  const at = authority.lastIndexOf(`@`)
  if (at >= 0) authority = authority.slice(at + 1)
  const close = authority.lastIndexOf(`]`)
  const colon = authority.lastIndexOf(`:`)
  if (colon > close) authority = authority.slice(0, colon)
  return authority.toLowerCase()
}

/** What a picture's src renders as: `load` (ours, data or local), a `link`
 *  (another website: never fetched) or plain `text` (the alt, maybe empty). */
export function imageSource(src: string, alt: string, origin: string): ImageSource {
  const altText = alt.replace(EDGE_SPACE, ``)
  const plain: ImageSource = { kind: `text`, text: altText }
  const s = src.replace(EDGE_SPACE, ``)
  if (!s || NOT_URL.test(s)) return plain
  const lower = s.toLowerCase()

  if (lower.startsWith(`data:`)) return isLoadableData(lower) ? { kind: `load` } : plain
  if (IMAGE_SOURCE_POLICY.localSchemes.some((scheme) => lower.startsWith(scheme))) return { kind: `load` }

  if (s.startsWith(`/`) && !s.startsWith(`//`)) return isAttachmentPath(s) ? { kind: `load` } : plain

  const base = origin.replace(/\/+$/, ``).toLowerCase()
  if (base && lower.startsWith(`${base}/`) && isAttachmentPath(s.slice(base.length))) return { kind: `load` }

  let href: string
  let rest: string
  if (s.startsWith(`//`)) {
    href = `https:${s}`
    rest = s.slice(2)
  } else {
    const scheme = IMAGE_SOURCE_POLICY.linkSchemes.find((sc) => lower.startsWith(`${sc}//`))
    if (!scheme) return plain
    href = s
    rest = s.slice(scheme.length + 2)
  }
  const host = linkHost(rest)
  if (!host) return plain
  return { kind: `link`, href, text: altText || IMAGE_ACTIONS_COPY.linkHost.replace(`{host}`, host) }
}

/** True when the picture may be fetched at all. */
export function imageSourceAllowed(src: string, origin: string): boolean {
  return imageSource(src, ``, origin).kind === `load`
}
