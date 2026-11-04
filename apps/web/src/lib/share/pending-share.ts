import { isAcceptedImageContentType } from "@/lib/storage/issue-attachments"
import { shareChatText } from "@exp/domain-contract/share-targets"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-1635 (share-targets.json): the ONE share on its way into the app. The
// service worker stashes the manifest's `share_target` POST in the Cache API
// (`public/sw.js` owns the writing half and these keys), `/share` loads it,
// and it stays pending, through Back and a reload, until its destination took
// it: the composer's seed, or a filed issue. A text share that reached the
// server's fallback rides `/share`'s search params and is held in memory.

export const SHARE_CACHE_NAME = `exponential-share`
const SHARE_META_KEY = `/__share/meta`
const shareFileKey = (index: number) => `/__share/file/${index}`

export interface PendingShare {
  title: string
  text: string
  url: string
  files: File[]
}

interface StashedFile {
  name: string
  type: string
  lastModified: number
}

let held: PendingShare | null = null

const shareCaches = (): CacheStorage | null => (typeof caches === `undefined` ? null : caches)

export function shareIsEmpty(share: PendingShare): boolean {
  return !share.title.trim() && !share.text.trim() && !share.url.trim() && share.files.length === 0
}

/** Hold `share` as the pending one (the search-param fallback). */
export function holdPendingShare(share: PendingShare): void {
  held = share
}

async function readStashedShare(): Promise<PendingShare | null> {
  const storage = shareCaches()
  if (!storage) return null
  const cache = await storage.open(SHARE_CACHE_NAME)
  const meta = await cache.match(SHARE_META_KEY)
  if (!meta) return null
  const stashed = (await meta.json()) as Omit<PendingShare, `files`> & { files: StashedFile[] }
  const files = await Promise.all(
    stashed.files.map(async ({ name, type, lastModified }, index) => {
      const body = await cache.match(shareFileKey(index))
      return body ? new File([await body.blob()], name, { type, lastModified }) : null
    })
  )
  return {
    title: stashed.title,
    text: stashed.text,
    url: stashed.url,
    files: files.filter((file): file is File => file !== null),
  }
}

/** The pending share, null = none (or the cache cannot be read). */
export async function loadPendingShare(): Promise<PendingShare | null> {
  if (held) return held
  try {
    held = await readStashedShare()
  } catch {
    held = null
  }
  return held
}

/** The share reached its destination (or the viewer signed out). */
export async function clearPendingShare(): Promise<void> {
  held = null
  await shareCaches()?.delete(SHARE_CACHE_NAME).catch(() => false)
}

/** The composer's seed: `shareChatText`, the files as pending attachments,
 *  the agent row on `pick`. A share's title is the fixture's subject; with a
 *  file riding along it is a filename, images or not. */
export function shareChatSeed(share: PendingShare, pick: string): LaunchSeed {
  const text = shareChatText({
    text: share.text,
    subject: share.title,
    url: share.url,
    hasImages: share.files.length > 0,
  })
  return {
    issueIds: [],
    ...(text ? { text } : {}),
    files: share.files,
    pick,
  }
}

/** The New issue page's prefill, the natives' (Android `buildSharePrefill`,
 *  iOS `ShareItemExtractor`): the title = the text's first line (120 chars
 *  at most), else the subject; the description = the rest. */
export function shareIssuePrefill(share: PendingShare): { title: string; description: string } {
  const text = shareChatText({ text: share.text, url: share.url, hasImages: true })
  const subject = share.title.trim()
  if (!text) {
    return { title: share.files.length > 0 ? `` : subject.slice(0, 120), description: `` }
  }
  const title = text.split(`\n`, 1)[0]!.trim().slice(0, 120)
  const description = text === title ? `` : text.startsWith(title) ? text.slice(title.length).trim() : text
  return { title, description }
}

/** The images the Share to page previews. */
export function shareImages(share: PendingShare): File[] {
  return share.files.filter((file) => isAcceptedImageContentType(file.type))
}
