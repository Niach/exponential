// The web's native capabilities (#VAPP-265, E5): the host side of the Engine's
// TS twin in a browser. One provider per capability (photos, files, share,
// print, notifications), each answering its native tools with an MCP call
// result whose structuredContent matches `capabilities.json` `nativeOutputSchema`.
// The bridge (validation, fileIds, the BlobStore) is the Engine's; this file
// only sees native arguments (bytes as base64 `data`).
//
// USER ACTIVATION: browsers open pickers, the share sheet and the print dialog
// only within a user gesture. App UI must call through the broker inside the
// click's task (no awaited network round trip before the call); a picker opened
// without one answers `failed: …` (never hangs).

import {
  decodeBase64,
  encodeBase64,
  errorMessage,
  failed,
  isAbort,
  type McpResult,
  notFound,
  ok,
  unavailable,
} from "./mcp"
import type { DirectoryHandleLike, FileLike, NotificationLike, OpenPickerOptions, WebPlatform } from "./platform"
import { browserPlatform } from "./platform"

export * from "./mcp"
export * from "./platform"
export * from "./stores"

import type { NativeCapability } from "../tools"

export type { NativeCapability }

export interface WebCapabilityOptions {
  /** A notification was tapped: `tap` = the lifecycle event posted with it, verbatim. */
  onTap?(tap: unknown): void
}

/** The OPFS directory that is the app's photo library. */
export const PHOTOS_DIR = `photos`

const MIME_BY_EXTENSION: Record<string, string> = {
  avif: `image/avif`,
  bmp: `image/bmp`,
  csv: `text/csv`,
  gif: `image/gif`,
  heic: `image/heic`,
  heif: `image/heif`,
  html: `text/html`,
  jpeg: `image/jpeg`,
  jpg: `image/jpeg`,
  json: `application/json`,
  md: `text/markdown`,
  pdf: `application/pdf`,
  png: `image/png`,
  svg: `image/svg+xml`,
  tif: `image/tiff`,
  tiff: `image/tiff`,
  txt: `text/plain`,
  webp: `image/webp`,
  zip: `application/zip`,
}

const OCTET = `application/octet-stream`
/** setTimeout's ceiling; later times chain. */
const MAX_DELAY_MS = 2_147_483_647

const extensionMime = (name: string): string | undefined => {
  const dot = name.lastIndexOf(`.`)
  return dot < 0 ? undefined : MIME_BY_EXTENSION[name.slice(dot + 1).toLowerCase()]
}

const codePoints = (text: string, max: number): string => {
  const chars = Array.from(text)
  return chars.length > max ? chars.slice(0, max).join(``) : text
}

const fileName = (name: string): string => codePoints(name || `file`, 255)

const fileMime = (file: { name: string; type: string }): string => {
  const type = file.type && file.type.length <= 100 ? file.type : undefined
  return type ?? extensionMime(file.name) ?? OCTET
}

const str = (args: Record<string, unknown>, key: string): string | undefined =>
  typeof args[key] === `string` ? (args[key] as string) : undefined

const bytesOf = async (file: FileLike): Promise<Uint8Array> => new Uint8Array(await file.arrayBuffer())

const blobOf = (bytes: Uint8Array, type: string): Blob => new Blob([bytes as Uint8Array<ArrayBuffer>], { type })

/** `accept` (MIME types or `.ext`) as `showOpenFilePicker` types. */
export const pickerTypes = (accept: string[]): OpenPickerOptions[`types`] => {
  const map: Record<string, string[]> = {}
  for (const raw of accept) {
    const entry = raw.trim()
    if (entry.includes(`/`)) {
      map[entry] ??= []
    } else {
      const ext = entry.startsWith(`.`) ? entry : `.${entry}`
      const mime = extensionMime(ext) ?? OCTET
      ;(map[mime] ??= []).push(ext)
    }
  }
  return [{ description: `Files`, accept: map }]
}

/** The shared picker: File System Access when present (TypeError on `accept` = the input), else a hidden input. */
const pickFiles = async (
  platform: WebPlatform,
  accept: string[],
  multiple: boolean,
): Promise<FileLike[]> => {
  if (platform.showOpenFilePicker) {
    const options: OpenPickerOptions = { multiple }
    if (accept.length > 0) {
      options.types = pickerTypes(accept)
      options.excludeAcceptAllOption = true
    }
    try {
      const handles = await platform.showOpenFilePicker(options)
      return await Promise.all(handles.map((h) => h.getFile()))
    } catch (error) {
      if (isAbort(error)) return []
      if (!(error instanceof TypeError)) throw error
    }
  }
  return platform.pickWithInput({ accept: accept.join(`,`), multiple })
}

const guard =
  (tools: Record<string, (args: Record<string, unknown>) => Promise<McpResult>>) =>
  async (tool: string, args: Record<string, unknown>): Promise<McpResult> => {
    const run = Object.hasOwn(tools, tool) ? tools[tool] : undefined
    if (!run) return unavailable()
    try {
      return await run(args ?? {})
    } catch (error) {
      return failed(errorMessage(error))
    }
  }

// ---- files ----

const filesCapability = (platform: WebPlatform): NativeCapability => ({
  name: `files`,
  call: guard({
    open: async (args) => {
      const accept = Array.isArray(args.accept) ? (args.accept as string[]) : []
      const files = await pickFiles(platform, accept, args.multiple === true)
      const picked = (args.multiple === true ? files : files.slice(0, 1)).slice(0, 100)
      return ok({
        files: await Promise.all(
          picked.map(async (f) => ({ name: fileName(f.name), mimeType: fileMime(f), data: encodeBase64(await bytesOf(f)) })),
        ),
      })
    },
    save: async (args) => {
      const name = str(args, `name`) ?? `file`
      const bytes = decodeBase64(str(args, `data`) ?? ``)
      if (!bytes) return failed(`bad base64`)
      const blob = blobOf(bytes, str(args, `mimeType`) ?? OCTET)
      if (platform.showSaveFilePicker) {
        let handle
        try {
          handle = await platform.showSaveFilePicker({ suggestedName: name })
        } catch (error) {
          if (isAbort(error)) return ok({ saved: false })
          if (!(error instanceof TypeError)) throw error
        }
        if (handle) {
          const writable = await handle.createWritable()
          await writable.write(blob)
          await writable.close()
          return ok({ saved: true })
        }
      }
      platform.download(name, blob)
      return ok({ saved: true })
    },
  }),
})

// ---- photos ----

interface LibraryEntry {
  id: string
  name: string
  takenMs: number
}

/** `<timestamp>-<name>`: the timestamp = the picked file's lastModified (ms). */
const parsePhotoId = (id: string): LibraryEntry | null => {
  const m = /^(\d{1,15})-(.+)$/.exec(id)
  if (!m) return null
  return { id, name: m[2]!, takenMs: Number(m[1]) }
}

const takenAt = (ms: number): string | null => (ms > 0 && ms <= 8.64e15 ? new Date(ms).toISOString() : null)

const photoMeta = (entry: LibraryEntry, mime?: string) => ({
  id: entry.id,
  name: fileName(entry.name),
  mimeType: mime ?? extensionMime(entry.name) ?? OCTET,
  takenAt: takenAt(entry.takenMs),
})

const safeName = (name: string): string => {
  const cleaned = codePoints(name.replace(/[/\\\u0000]/g, `_`), 200)
  return cleaned === `` || cleaned === `.` || cleaned === `..` ? `photo` : cleaned
}

const photosCapability = (platform: WebPlatform): NativeCapability => {
  const library = async (): Promise<DirectoryHandleLike | null> => {
    if (!platform.storageDirectory) return null
    const root = await platform.storageDirectory()
    return root.getDirectoryHandle(PHOTOS_DIR, { create: true })
  }
  const entries = async (dir: DirectoryHandleLike): Promise<LibraryEntry[]> => {
    const out: LibraryEntry[] = []
    for await (const handle of dir.values()) {
      if (handle.kind !== `file`) continue
      const entry = parsePhotoId(handle.name)
      if (entry) out.push(entry)
    }
    return out.sort((a, b) => b.takenMs - a.takenMs || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0))
  }
  return {
    name: `photos`,
    call: guard({
      pick: async (args) => {
        const limit = typeof args.limit === `number` ? Math.max(1, Math.min(50, Math.floor(args.limit))) : 1
        const files = (await pickFiles(platform, [`image/*`], limit > 1)).slice(0, limit)
        const dir = files.length > 0 ? await library().catch(() => null) : null
        const photos = []
        for (const file of files) {
          const bytes = await bytesOf(file)
          const ms = Number.isSafeInteger(file.lastModified) && file.lastModified > 0 ? file.lastModified : 0
          const id = `${ms}-${safeName(file.name)}`
          if (dir) {
            try {
              const writable = await (await dir.getFileHandle(id, { create: true })).createWritable()
              await writable.write(bytes)
              await writable.close()
            } catch {
              // The pick still answers; `get` of this id answers notFound.
            }
          }
          const meta = photoMeta({ id, name: safeName(file.name), takenMs: ms }, fileMime(file))
          photos.push({ ...meta, data: encodeBase64(bytes) })
        }
        return ok({ photos })
      },
      list: async (args) => {
        const limit = typeof args.limit === `number` ? Math.max(1, Math.min(100, Math.floor(args.limit))) : 50
        const cursor = str(args, `cursor`)
        if (cursor !== undefined && !/^\d{1,9}$/.test(cursor)) return failed(`bad cursor`)
        const dir = await library().catch(() => null)
        if (!dir) return ok({ photos: [], cursor: null })
        const all = await entries(dir)
        const offset = cursor === undefined ? 0 : Number(cursor)
        const page = all.slice(offset, offset + limit)
        const next = offset + limit < all.length ? String(offset + limit) : null
        return ok({ photos: page.map((e) => photoMeta(e)), cursor: next })
      },
      get: async (args) => {
        const entry = parsePhotoId(str(args, `id`) ?? ``)
        if (!entry) return notFound()
        const dir = await library().catch(() => null)
        if (!dir) return notFound()
        let file: FileLike
        try {
          file = await (await dir.getFileHandle(entry.id)).getFile()
        } catch {
          return notFound()
        }
        return ok({ photo: { ...photoMeta(entry), data: encodeBase64(await bytesOf(file)) } })
      },
    }),
  }
}

// ---- share ----

const shareCapability = (platform: WebPlatform): NativeCapability => ({
  name: `share`,
  call: guard({
    send: async (args) => {
      if (!platform.share) return unavailable()
      const data: { title?: string; text?: string; url?: string; files?: File[] } = {}
      for (const key of [`title`, `text`, `url`] as const) {
        const value = str(args, key)
        if (value !== undefined) data[key] = value
      }
      const file = args.file as { name?: string; mimeType?: string; data?: string } | undefined | null
      if (file && typeof file === `object`) {
        const bytes = decodeBase64(file.data ?? ``)
        if (!bytes) return failed(`bad base64`)
        const shared = new File([bytes as Uint8Array<ArrayBuffer>], file.name ?? `file`, { type: file.mimeType ?? OCTET })
        if (platform.canShare?.({ files: [shared] })) data.files = [shared]
        else if (data.text === undefined && data.url === undefined) return failed(`this browser cannot share files`)
      }
      try {
        await platform.share(data)
      } catch (error) {
        if (isAbort(error)) return ok({ shared: false })
        throw error
      }
      return ok({ shared: true })
    },
  }),
})

// ---- print ----

const printCapability = (platform: WebPlatform): NativeCapability => ({
  name: `print`,
  call: guard({
    // `printed` is always true: a browser never tells whether the person printed or cancelled.
    print: async (args) => {
      const bytes = decodeBase64(str(args, `data`) ?? ``)
      if (!bytes) return failed(`bad base64`)
      await platform.print(blobOf(bytes, str(args, `mimeType`) ?? `application/pdf`))
      return ok({ printed: true })
    },
  }),
})

// ---- notifications ----

interface Posted {
  timer?: unknown
  shown?: NotificationLike
}

/**
 * Notifications live only while the page is open: a scheduled one is a
 * setTimeout (no service worker), so it fires only if the page is still open
 * at `at`; one shown is withdrawn by `cancel` or a re-post of its id (`tag`).
 */
const notificationsCapability = (platform: WebPlatform, opts: WebCapabilityOptions): NativeCapability => {
  const api = platform.notifications!
  const posted = new Map<string, Posted>()

  const withdraw = (id: string): boolean => {
    const entry = posted.get(id)
    if (!entry) return false
    posted.delete(id)
    if (entry.timer !== undefined) platform.clearTimeout(entry.timer)
    if (entry.shown) {
      entry.shown.onclose = null
      entry.shown.close()
    }
    return true
  }

  const show = (id: string, entry: Posted, title: string, body: string | null, tap: unknown) => {
    entry.timer = undefined
    const n = api.show(title, body === null ? { tag: id } : { body, tag: id })
    entry.shown = n
    n.onclick = () => {
      platform.focus()
      if (tap !== null && tap !== undefined) opts.onTap?.(tap)
      withdraw(id)
    }
    n.onclose = () => {
      if (posted.get(id) === entry) posted.delete(id)
    }
  }

  const schedule = (id: string, entry: Posted, atMs: number, fire: () => void) => {
    const delay = atMs - platform.now()
    if (delay <= 0) return fire()
    entry.timer = platform.setTimeout(() => {
      if (posted.get(id) === entry) schedule(id, entry, atMs, fire)
    }, Math.min(delay, MAX_DELAY_MS))
  }

  return {
    name: `notifications`,
    call: guard({
      post: async (args) => {
        const id = str(args, `id`) ?? ``
        const title = str(args, `title`) ?? ``
        const body = str(args, `body`) ?? null
        const at = str(args, `at`)
        const atMs = at === undefined ? Number.NaN : Date.parse(at)
        if (at !== undefined && Number.isNaN(atMs)) return failed(`bad at`)
        let permission = api.permission()
        if (permission === `default`) permission = await api.requestPermission()
        if (permission !== `granted`) return failed(`notifications blocked`)
        withdraw(id)
        const entry: Posted = {}
        posted.set(id, entry)
        const tap = args.tap ?? null
        const fire = () => {
          if (posted.get(id) === entry) show(id, entry, title, body, tap)
        }
        if (Number.isNaN(atMs)) fire()
        else schedule(id, entry, atMs, fire)
        return ok({})
      },
      cancel: async (args) => ok({ cancelled: withdraw(str(args, `id`) ?? ``) }),
    }),
  }
}

/**
 * The web's native capabilities, leaving out what this browser lacks
 * (no `navigator.share` = no share, no Notification = no notifications).
 * photos, files and print are always there (photos without OPFS = an empty library).
 */
export const webCapabilities = (
  platform: WebPlatform = browserPlatform(),
  opts: WebCapabilityOptions = {},
): NativeCapability[] => {
  const out = [photosCapability(platform), filesCapability(platform)]
  if (platform.share) out.push(shareCapability(platform))
  out.push(printCapability(platform))
  if (platform.notifications) out.push(notificationsCapability(platform, opts))
  return out
}
