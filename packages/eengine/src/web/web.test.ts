// The web capabilities against fakes of every browser API (#VAPP-265).
// Each structuredContent is checked against capabilities.json `nativeOutputSchema`.

import { describe, expect, test } from "bun:test"
import capabilities from "../../fixtures/capabilities.json" with { type: "json" }
import providers from "../../fixtures/providers.json" with { type: "json" }
import {
  decodeBase64,
  type DirectoryHandleLike,
  encodeBase64,
  type FileHandleLike,
  type NativeCapability,
  type NotificationLike,
  type OpenPickerOptions,
  pickerTypes,
  type ShareDataLike,
  type WebPlatform,
  webCapabilities,
} from "./index"

type Schema = Record<string, unknown>
const tools = (capabilities as unknown as { tools: { name: string; runs: string; nativeOutputSchema?: Schema }[] }).tools

// ---- a minimal checker for the keywords nativeOutputSchema uses ----

const DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/
const typeOf = (v: unknown) =>
  v === null ? `null` : Array.isArray(v) ? `array` : Number.isInteger(v) ? `integer` : typeof v

const check = (schema: Schema, value: unknown, path = ``): string | null => {
  const at = path || `/`
  if (schema.type !== undefined) {
    const types = Array.isArray(schema.type) ? (schema.type as string[]) : [schema.type as string]
    const t = typeOf(value)
    if (!types.some((s) => s === t || (s === `number` && t === `integer`))) return `${at} type`
  }
  if (typeof value === `string`) {
    const n = Array.from(value).length
    if (typeof schema.minLength === `number` && n < schema.minLength) return `${at} minLength`
    if (typeof schema.maxLength === `number` && n > schema.maxLength) return `${at} maxLength`
    if (schema.format === `date-time` && !DATE_TIME.test(value)) return `${at} format`
    if (schema.format === `base64` && decodeBase64(value) === null) return `${at} format`
    if (schema.format === `token` && !/^[A-Za-z0-9._-]{1,64}$/.test(value)) return `${at} format`
  }
  if (typeof value === `number` && typeof schema.minimum === `number` && value < schema.minimum) return `${at} minimum`
  if (Array.isArray(value)) {
    if (typeof schema.minItems === `number` && value.length < schema.minItems) return `${at} minItems`
    if (typeof schema.maxItems === `number` && value.length > schema.maxItems) return `${at} maxItems`
    for (let i = 0; i < value.length; i++) {
      const e = schema.items ? check(schema.items as Schema, value[i], `${path}/${i}`) : null
      if (e) return e
    }
  }
  if (typeOf(value) === `object`) {
    const obj = value as Record<string, unknown>
    const props = (schema.properties ?? {}) as Record<string, Schema>
    for (const key of (schema.required ?? []) as string[]) if (!(key in obj)) return `${path}/${key} required`
    if (schema.additionalProperties === false)
      for (const key of Object.keys(obj).sort()) if (!(key in props)) return `${path}/${key} additionalProperties`
    for (const [key, sub] of Object.entries(props)) {
      if (!(key in obj)) continue
      const e = check(sub, obj[key], `${path}/${key}`)
      if (e) return e
    }
  }
  return null
}

type Result = { content: { type: string; text: string }[]; structuredContent?: Record<string, unknown>; isError?: boolean }

/** A flat `<capability>_<tool>` splits at its first `_`. */
const splitTool = (name: string): [string, string] => {
  const at = name.indexOf(`_`)
  return [name.slice(0, at), name.slice(at + 1)]
}

/** Calls `<cap>_<tool>`, asserts the MCP result shape and the fixture schema, answers structuredContent. */
const okCall = async (caps: NativeCapability[], name: string, args: Record<string, unknown> = {}) => {
  const [cap, tool] = splitTool(name)
  const provider = caps.find((c) => c.name === cap)
  if (!provider) throw new Error(`no ${cap}`)
  const result = (await provider.call(tool, args)) as Result
  expect(result.isError).toBeUndefined()
  expect(result.content).toEqual([{ type: `text`, text: JSON.stringify(result.structuredContent) }])
  const schema = tools.find((t) => t.name === name)!.nativeOutputSchema!
  expect(check(schema, result.structuredContent)).toBeNull()
  return result.structuredContent!
}

const errCall = async (caps: NativeCapability[], name: string, args: Record<string, unknown> = {}) => {
  const [cap, tool] = splitTool(name)
  const result = (await caps.find((c) => c.name === cap)!.call(tool, args)) as Result
  expect(result.isError).toBe(true)
  expect(result.structuredContent).toBeUndefined()
  expect(result.content.length).toBe(1)
  return result.content[0]!.text
}

// ---- fakes ----

const abort = () => Object.assign(new Error(`cancelled`), { name: `AbortError` })
const bytes = (text: string) => new TextEncoder().encode(text)
const file = (name: string, text: string, type = ``, lastModified = 0) =>
  new File([bytes(text)], name, { type, lastModified })

const memoryWritable = (onClose: (data: Uint8Array) => void) => {
  const chunks: Uint8Array[] = []
  return {
    write: async (data: Blob | Uint8Array) => {
      chunks.push(data instanceof Uint8Array ? data : new Uint8Array(await data.arrayBuffer()))
    },
    close: async () => {
      const total = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0))
      let o = 0
      for (const c of chunks) {
        total.set(c, o)
        o += c.length
      }
      onClose(total)
    },
  }
}

const memoryFileHandle = (name: string, store: Map<string, Uint8Array>): FileHandleLike => ({
  kind: `file`,
  name,
  getFile: async () => {
    const data = store.get(name)
    if (!data) throw Object.assign(new Error(`gone`), { name: `NotFoundError` })
    return new File([data as Uint8Array<ArrayBuffer>], name, { lastModified: 1 })
  },
  createWritable: async () => memoryWritable((data) => store.set(name, data)),
})

/** An in-memory OPFS: dirs by name, files by name. */
const memoryDirectory = (name = ``): DirectoryHandleLike & { files: Map<string, Uint8Array> } => {
  const files = new Map<string, Uint8Array>()
  const dirs = new Map<string, ReturnType<typeof memoryDirectory>>()
  return {
    kind: `directory`,
    name,
    files,
    getDirectoryHandle: async (n, options) => {
      let d = dirs.get(n)
      if (!d && options?.create) dirs.set(n, (d = memoryDirectory(n)))
      if (!d) throw Object.assign(new Error(`no dir`), { name: `NotFoundError` })
      return d
    },
    getFileHandle: async (n, options) => {
      if (n.includes(`/`) || n === `..`) throw new TypeError(`bad name`)
      if (!files.has(n)) {
        if (!options?.create) throw Object.assign(new Error(`no file`), { name: `NotFoundError` })
        files.set(n, new Uint8Array())
      }
      return memoryFileHandle(n, files)
    },
    async *values() {
      for (const d of dirs.values()) yield d
      for (const n of files.keys()) yield memoryFileHandle(n, files)
    },
  }
}

interface Fake extends WebPlatform {
  log: string[]
  inputAnswer: File[]
  openAnswer: File[] | `abort` | `typeError`
  saveAnswer: `abort` | Map<string, Uint8Array>
  shareAnswer: `ok` | `abort` | `denied`
  shared: ShareDataLike[]
  shareFiles: boolean
  printed: Blob[]
  downloads: { name: string; blob: Blob }[]
  openOptions: OpenPickerOptions[]
  inputOptions: { accept: string; multiple: boolean }[]
  root: ReturnType<typeof memoryDirectory>
  clock: number
  timers: Map<number, { fn: () => void; at: number }>
  advance(ms: number): void
  permission: `default` | `granted` | `denied`
  permissionAfterAsk: `default` | `granted` | `denied`
  shown: (NotificationLike & { title: string; body?: string; tag: string; closed: boolean })[]
}

const fake = (over: { fsAccess?: boolean; share?: boolean; opfs?: boolean; notifications?: boolean } = {}): Fake => {
  const f = {
    log: [],
    inputAnswer: [],
    openAnswer: [],
    saveAnswer: new Map(),
    shareAnswer: `ok`,
    shared: [],
    shareFiles: true,
    printed: [],
    downloads: [],
    openOptions: [],
    inputOptions: [],
    root: memoryDirectory(),
    clock: Date.parse(`2026-11-06T10:00:00Z`),
    timers: new Map(),
    permission: `granted`,
    permissionAfterAsk: `granted`,
    shown: [],
    pickWithInput: async (options) => {
      f.inputOptions.push(options)
      f.log.push(`input`)
      return f.inputAnswer
    },
    download: (name, blob) => {
      f.downloads.push({ name, blob })
    },
    print: async (blob) => {
      f.printed.push(blob)
    },
    now: () => f.clock,
    setTimeout: (fn, ms) => {
      const id = f.timers.size + 1 + Math.random()
      f.timers.set(id, { fn, at: f.clock + ms })
      return id
    },
    clearTimeout: (handle) => {
      f.timers.delete(handle as number)
    },
    advance: (ms) => {
      const end = f.clock + ms
      for (;;) {
        const due = [...f.timers.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0]
        if (!due) break
        f.timers.delete(due[0])
        f.clock = due[1].at
        due[1].fn()
      }
      f.clock = end
    },
    focus: () => {
      f.log.push(`focus`)
    },
  } as Fake
  if (over.fsAccess !== false) {
    f.showOpenFilePicker = async (options) => {
      f.openOptions.push(options)
      f.log.push(`picker`)
      if (f.openAnswer === `abort`) throw abort()
      if (f.openAnswer === `typeError`) throw new TypeError(`bad accept`)
      const answer = f.openAnswer
      return answer.map((file) => ({
        kind: `file` as const,
        name: file.name,
        getFile: async () => file,
        createWritable: async () => {
          throw new Error(`read only`)
        },
      }))
    }
    f.showSaveFilePicker = async ({ suggestedName }) => {
      if (f.saveAnswer === `abort`) throw abort()
      return memoryFileHandle(suggestedName, f.saveAnswer)
    }
  }
  if (over.share !== false) {
    f.share = async (data) => {
      if (f.shareAnswer === `abort`) throw abort()
      if (f.shareAnswer === `denied`) throw Object.assign(new Error(`Must be handling a user gesture`), { name: `NotAllowedError` })
      f.shared.push(data)
    }
    f.canShare = (data) => f.shareFiles || !data.files
  }
  if (over.opfs !== false) f.storageDirectory = async () => f.root
  if (over.notifications !== false) {
    f.notifications = {
      permission: () => f.permission,
      requestPermission: async () => {
        f.log.push(`ask`)
        f.permission = f.permissionAfterAsk
        return f.permission
      },
      show: (title, options) => {
        const n = {
          title,
          ...options,
          closed: false,
          onclick: null,
          onclose: null,
          close() {
            if (n.closed) return
            n.closed = true
            n.onclose?.({})
          },
        } as Fake[`shown`][number]
        f.shown.push(n)
        return n
      },
    }
  }
  return f
}

// ---- tests ----

describe(`base64`, () => {
  test(`round trips and refuses the lenient forms`, () => {
    for (const s of [``, `h`, `he`, `hel`, `hello`, `hello!`]) {
      const enc = encodeBase64(bytes(s))
      expect(enc).toBe(Buffer.from(s).toString(`base64`))
      expect(new TextDecoder().decode(decodeBase64(enc)!)).toBe(s)
    }
    const all = new Uint8Array(256).map((_, i) => i)
    expect(encodeBase64(all)).toBe(Buffer.from(all).toString(`base64`))
    for (const bad of [`aGVsbG8`, `aGVs bG8=`, `aGV-bG8=`, `aGVsbG9=`, `a===`, `=AAA`, `aGVsbG8=\n`]) expect(decodeBase64(bad)).toBeNull()
  })
})

describe(`webCapabilities`, () => {
  test(`mounts what the browser has`, () => {
    expect(webCapabilities(fake()).map((c) => c.name)).toEqual([`photos`, `files`, `share`, `print`, `notifications`])
    expect(webCapabilities(fake({ share: false, notifications: false })).map((c) => c.name)).toEqual([
      `photos`,
      `files`,
      `print`,
    ])
  })

  test(`every native tool of a mounted capability answers; an unknown one is unavailable`, async () => {
    const caps = webCapabilities(fake())
    // #VAPP-273: a capability only eDE's shell provides (windows, focus, launch, shell) has no web native.
    const hosts = providers.hosts as Record<string, { capabilities?: { name: string }[] }>
    const edeOnly = (hosts.ede!.capabilities ?? []).map((c) => c.name).filter((n) => !Object.entries(hosts).some(([h, c]) => h !== `ede` && (c.capabilities ?? []).some((x) => x.name === n)))
    expect(edeOnly).toEqual([`shell`, `windows`, `focus`, `launch`])
    for (const t of tools.filter((t) => t.runs === `native`)) {
      const [cap] = splitTool(t.name)
      expect(caps.some((c) => c.name === cap) || edeOnly.includes(cap!)).toBe(true)
    }
    expect(await errCall(caps, `print_pdf`)).toBe(`unavailable`)
    expect(await errCall(caps, `files_toString`)).toBe(`unavailable`)
  })
})

describe(`files`, () => {
  test(`open through File System Access: accept as picker types, bytes as base64`, async () => {
    const f = fake()
    f.openAnswer = [file(`a.csv`, `x,y`, `text/csv`), file(`b.bin`, `hello`)]
    const caps = webCapabilities(f)
    const out = await okCall(caps, `files_open`, { accept: [`image/*`, `.csv`, `.bin`], multiple: true })
    expect(out).toEqual({
      files: [
        { name: `a.csv`, mimeType: `text/csv`, data: encodeBase64(bytes(`x,y`)) },
        { name: `b.bin`, mimeType: `application/octet-stream`, data: `aGVsbG8=` },
      ],
    })
    expect(f.openOptions[0]).toEqual({
      multiple: true,
      excludeAcceptAllOption: true,
      types: [{ description: `Files`, accept: { "image/*": [], "text/csv": [`.csv`], "application/octet-stream": [`.bin`] } }],
    })
    expect(f.log).toEqual([`picker`])
  })

  test(`open single keeps one file; cancel = no files`, async () => {
    const f = fake()
    f.openAnswer = [file(`a.txt`, `1`), file(`b.txt`, `2`)]
    const caps = webCapabilities(f)
    expect(((await okCall(caps, `files_open`, { multiple: false })).files as unknown[]).length).toBe(1)
    expect(f.openOptions[0]).toEqual({ multiple: false })
    f.openAnswer = `abort`
    expect(await okCall(caps, `files_open`, { multiple: true })).toEqual({ files: [] })
  })

  test(`open without File System Access (or a refused accept) = the hidden input; its cancel = no files`, async () => {
    const f = fake({ fsAccess: false })
    f.inputAnswer = [file(`p.png`, `png`)]
    const caps = webCapabilities(f)
    expect(await okCall(caps, `files_open`, { accept: [`image/png`, `.png`], multiple: false })).toEqual({
      files: [{ name: `p.png`, mimeType: `image/png`, data: encodeBase64(bytes(`png`)) }],
    })
    expect(f.inputOptions).toEqual([{ accept: `image/png,.png`, multiple: false }])
    f.inputAnswer = []
    expect(await okCall(caps, `files_open`, { multiple: false })).toEqual({ files: [] })

    const g = fake()
    g.openAnswer = `typeError`
    g.inputAnswer = [file(`x.txt`, `x`)]
    expect(((await okCall(webCapabilities(g), `files_open`, { accept: [`weird`], multiple: false })).files as unknown[]).length).toBe(1)
    expect(g.log).toEqual([`picker`, `input`])
  })

  test(`open: a picker refusal (no gesture) is failed`, async () => {
    const f = fake({ fsAccess: false })
    f.pickWithInput = async () => {
      throw new Error(`the picker needs a user gesture`)
    }
    expect(await errCall(webCapabilities(f), `files_open`, { multiple: false })).toBe(`failed: the picker needs a user gesture`)
  })

  test(`save through the save picker, cancel = saved false, else a download`, async () => {
    const f = fake()
    const store = new Map<string, Uint8Array>()
    f.saveAnswer = store
    const caps = webCapabilities(f)
    const args = { name: `r.txt`, mimeType: `text/plain`, data: `aGVsbG8=` }
    expect(await okCall(caps, `files_save`, args)).toEqual({ saved: true })
    expect(new TextDecoder().decode(store.get(`r.txt`)!)).toBe(`hello`)
    f.saveAnswer = `abort`
    expect(await okCall(caps, `files_save`, args)).toEqual({ saved: false })

    const g = fake({ fsAccess: false })
    expect(await okCall(webCapabilities(g), `files_save`, args)).toEqual({ saved: true })
    expect(g.downloads[0]!.name).toBe(`r.txt`)
    expect(g.downloads[0]!.blob.type).toStartWith(`text/plain`)
    expect(await g.downloads[0]!.blob.text()).toBe(`hello`)
    expect(await errCall(webCapabilities(g), `files_save`, { ...args, data: `aGVsbG8` })).toBe(`failed: bad base64`)
  })

  test(`pickerTypes`, () => {
    expect(pickerTypes([`.JPG`, `pdf`])).toEqual([{ description: `Files`, accept: { "image/jpeg": [`.JPG`], "application/pdf": [`.pdf`] } }])
  })
})

describe(`photos`, () => {
  test(`pick copies into OPFS; list (newest first, paged) and get work after a reload`, async () => {
    const f = fake()
    f.openAnswer = [
      file(`old.jpg`, `old`, `image/jpeg`, Date.parse(`2026-01-01T00:00:00Z`)),
      file(`new.png`, `new`, `image/png`, Date.parse(`2026-06-01T12:00:00Z`)),
      file(`mid.webp`, `mid`, ``, Date.parse(`2026-03-01T00:00:00Z`)),
    ]
    const picked = await okCall(webCapabilities(f), `photos_pick`, { limit: 3 })
    expect(f.openOptions[0]!.types).toEqual([{ description: `Files`, accept: { "image/*": [] } }])
    expect(picked.photos).toEqual([
      { id: `${Date.parse(`2026-01-01T00:00:00Z`)}-old.jpg`, name: `old.jpg`, mimeType: `image/jpeg`, takenAt: `2026-01-01T00:00:00.000Z`, data: encodeBase64(bytes(`old`)) },
      { id: `${Date.parse(`2026-06-01T12:00:00Z`)}-new.png`, name: `new.png`, mimeType: `image/png`, takenAt: `2026-06-01T12:00:00.000Z`, data: encodeBase64(bytes(`new`)) },
      { id: `${Date.parse(`2026-03-01T00:00:00Z`)}-mid.webp`, name: `mid.webp`, mimeType: `image/webp`, takenAt: `2026-03-01T00:00:00.000Z`, data: encodeBase64(bytes(`mid`)) },
    ])

    // A reload: new capabilities, the same OPFS.
    const caps = webCapabilities({ ...fake(), storageDirectory: f.storageDirectory })
    const first = await okCall(caps, `photos_list`, { limit: 2 })
    expect((first.photos as { name: string }[]).map((p) => p.name)).toEqual([`new.png`, `mid.webp`])
    expect(first.photos).toContainEqual({ id: `${Date.parse(`2026-06-01T12:00:00Z`)}-new.png`, name: `new.png`, mimeType: `image/png`, takenAt: `2026-06-01T12:00:00.000Z` })
    expect(first.cursor).toBe(`2`)
    const second = await okCall(caps, `photos_list`, { limit: 2, cursor: first.cursor })
    expect((second.photos as { name: string }[]).map((p) => p.name)).toEqual([`old.jpg`])
    expect(second.cursor).toBeNull()

    const id = (picked.photos as { id: string }[])[1]!.id
    expect(await okCall(caps, `photos_get`, { id })).toEqual({
      photo: { id, name: `new.png`, mimeType: `image/png`, takenAt: `2026-06-01T12:00:00.000Z`, data: encodeBase64(bytes(`new`)) },
    })
    expect(await errCall(caps, `photos_get`, { id: `1-nope.jpg` })).toBe(`notFound`)
    expect(await errCall(caps, `photos_get`, { id: `../etc` })).toBe(`notFound`)
    expect(await errCall(caps, `photos_list`, { limit: 2, cursor: `abc` })).toBe(`failed: bad cursor`)
  })

  test(`pick honours limit, a zero lastModified is a null takenAt, cancel = no photos`, async () => {
    const f = fake({ fsAccess: false })
    f.inputAnswer = [file(`a/b.gif`, `1`, `image/gif`), file(`c.gif`, `2`, `image/gif`)]
    const caps = webCapabilities(f)
    const out = await okCall(caps, `photos_pick`, { limit: 1 })
    expect(out.photos).toEqual([{ id: `0-a_b.gif`, name: `a_b.gif`, mimeType: `image/gif`, takenAt: null, data: `MQ==` }])
    expect(f.inputOptions[0]).toEqual({ accept: `image/*`, multiple: false })
    f.inputAnswer = []
    expect(await okCall(caps, `photos_pick`, { limit: 5 })).toEqual({ photos: [] })
  })

  test(`without OPFS: pick still answers, the library is empty, get is notFound`, async () => {
    const f = fake({ opfs: false })
    f.openAnswer = [file(`a.png`, `1`, `image/png`, 5)]
    const caps = webCapabilities(f)
    const out = await okCall(caps, `photos_pick`, { limit: 1 })
    expect(await okCall(caps, `photos_list`, { limit: 50 })).toEqual({ photos: [], cursor: null })
    expect(await errCall(caps, `photos_get`, { id: (out.photos as { id: string }[])[0]!.id })).toBe(`notFound`)
  })
})

describe(`share`, () => {
  test(`sends title, text, url and a shareable file; abort = shared false; a refusal is failed`, async () => {
    const f = fake()
    const caps = webCapabilities(f)
    expect(
      await okCall(caps, `share_send`, { title: `T`, text: `hi`, url: `https://x.at/`, file: { name: `a.txt`, mimeType: `text/plain`, data: `aGVsbG8=` } }),
    ).toEqual({ shared: true })
    const sent = f.shared[0]!
    expect([sent.title, sent.text, sent.url]).toEqual([`T`, `hi`, `https://x.at/`])
    expect(sent.files!.length).toBe(1)
    expect(sent.files![0]!.name).toBe(`a.txt`)
    expect(await sent.files![0]!.text()).toBe(`hello`)

    f.shareFiles = false
    expect(await okCall(caps, `share_send`, { url: `https://x.at/`, file: { name: `a.txt`, mimeType: `text/plain`, data: `aGVsbG8=` } })).toEqual({ shared: true })
    expect(f.shared[1]!.files).toBeUndefined()
    expect(await errCall(caps, `share_send`, { file: { name: `a.txt`, mimeType: `text/plain`, data: `aGVsbG8=` } })).toBe(
      `failed: this browser cannot share files`,
    )

    f.shareAnswer = `abort`
    expect(await okCall(caps, `share_send`, { text: `hi` })).toEqual({ shared: false })
    f.shareAnswer = `denied`
    expect(await errCall(caps, `share_send`, { text: `hi` })).toBe(`failed: Must be handling a user gesture`)
  })
})

describe(`print`, () => {
  test(`hands the bytes to the print frame; printed true`, async () => {
    const f = fake()
    const caps = webCapabilities(f)
    expect(await okCall(caps, `print_print`, { name: `d.pdf`, mimeType: `application/pdf`, data: `JVBERi0xLjQK` })).toEqual({ printed: true })
    expect(f.printed[0]!.type).toBe(`application/pdf`)
    expect(await f.printed[0]!.text()).toBe(`%PDF-1.4\n`)
    f.print = async () => {
      throw new Error(`no print frame`)
    }
    expect(await errCall(caps, `print_print`, { name: `d.pdf`, mimeType: `application/pdf`, data: `JVBERi0xLjQK` })).toBe(`failed: no print frame`)
  })
})

describe(`notifications`, () => {
  const tap = { type: `notificationTap`, app: `acme.album`, screen: `page`, params: { id: 7 } }

  test(`post now; a tap hands the event back verbatim and focuses the window`, async () => {
    const f = fake()
    const taps: unknown[] = []
    const caps = webCapabilities(f, { onTap: (t) => taps.push(t) })
    expect(await okCall(caps, `notifications_post`, { id: `acme.album/n1`, title: `Hi`, body: `there`, at: null, tap })).toEqual({})
    expect(f.shown.map((n) => [n.title, n.body, n.tag])).toEqual([[`Hi`, `there`, `acme.album/n1`]])
    f.shown[0]!.onclick!({})
    expect(taps).toEqual([tap])
    expect(taps[0]).toBe(tap)
    expect(f.log).toContain(`focus`)
    expect(f.shown[0]!.closed).toBe(true)
    expect(await okCall(caps, `notifications_cancel`, { id: `acme.album/n1` })).toEqual({ cancelled: false })
  })

  test(`a null tap only focuses; a null body is left out`, async () => {
    const f = fake()
    const taps: unknown[] = []
    const caps = webCapabilities(f, { onTap: (t) => taps.push(t) })
    await okCall(caps, `notifications_post`, { id: `a/x`, title: `T`, body: null, at: null, tap: null })
    expect(`body` in f.shown[0]!).toBe(false)
    f.shown[0]!.onclick!({})
    expect(taps).toEqual([])
    expect(f.log).toEqual([`focus`])
  })

  test(`asks when default; denied = failed: notifications blocked`, async () => {
    const f = fake()
    f.permission = `default`
    const caps = webCapabilities(f)
    await okCall(caps, `notifications_post`, { id: `a/x`, title: `T`, body: null, at: null, tap: null })
    expect(f.log).toEqual([`ask`])
    const g = fake()
    g.permission = `default`
    g.permissionAfterAsk = `denied`
    expect(await errCall(webCapabilities(g), `notifications_post`, { id: `a/x`, title: `T`, body: null, at: null, tap: null })).toBe(
      `failed: notifications blocked`,
    )
    const h = fake()
    h.permission = `denied`
    expect(await errCall(webCapabilities(h), `notifications_post`, { id: `a/x`, title: `T`, body: null, at: null, tap: null })).toBe(
      `failed: notifications blocked`,
    )
    expect(h.log).toEqual([])
  })

  test(`a future at is scheduled, a past one is now; cancel withdraws pending and shown`, async () => {
    const f = fake()
    const caps = webCapabilities(f)
    const inAMinute = new Date(f.clock + 60_000).toISOString()
    await okCall(caps, `notifications_post`, { id: `a/later`, title: `L`, body: null, at: inAMinute, tap: null })
    await okCall(caps, `notifications_post`, { id: `a/past`, title: `P`, body: null, at: `2020-01-01T00:00:00Z`, tap: null })
    expect(f.shown.map((n) => n.title)).toEqual([`P`])
    f.advance(59_999)
    expect(f.shown.length).toBe(1)
    f.advance(1)
    expect(f.shown.map((n) => n.title)).toEqual([`P`, `L`])

    await okCall(caps, `notifications_post`, { id: `a/pending`, title: `X`, body: null, at: inAMinute.replace(`10:01`, `11:00`), tap: null })
    expect(await okCall(caps, `notifications_cancel`, { id: `a/pending` })).toEqual({ cancelled: true })
    f.advance(10 * 3_600_000)
    expect(f.shown.map((n) => n.title)).toEqual([`P`, `L`])
    expect(f.timers.size).toBe(0)

    expect(await okCall(caps, `notifications_cancel`, { id: `a/past` })).toEqual({ cancelled: true })
    expect(f.shown[0]!.closed).toBe(true)
    expect(await okCall(caps, `notifications_cancel`, { id: `a/past` })).toEqual({ cancelled: false })
    expect(await okCall(caps, `notifications_cancel`, { id: `a/never` })).toEqual({ cancelled: false })
  })

  test(`re-posting an id replaces it (pending and shown); a closed one is gone`, async () => {
    const f = fake()
    const taps: unknown[] = []
    const caps = webCapabilities(f, { onTap: (t) => taps.push(t) })
    const later = new Date(f.clock + 1_000).toISOString()
    await okCall(caps, `notifications_post`, { id: `a/r`, title: `one`, body: null, at: later, tap: null })
    await okCall(caps, `notifications_post`, { id: `a/r`, title: `two`, body: null, at: null, tap })
    f.advance(5_000)
    expect(f.shown.map((n) => n.title)).toEqual([`two`])
    await okCall(caps, `notifications_post`, { id: `a/r`, title: `three`, body: null, at: null, tap })
    expect(f.shown.map((n) => [n.title, n.closed])).toEqual([
      [`two`, true],
      [`three`, false],
    ])
    f.shown[1]!.close()
    expect(await okCall(caps, `notifications_cancel`, { id: `a/r` })).toEqual({ cancelled: false })
    expect(taps).toEqual([])
  })

  test(`an at beyond setTimeout's ceiling chains timers`, async () => {
    const f = fake()
    const caps = webCapabilities(f)
    const far = new Date(f.clock + 40 * 86_400_000).toISOString()
    await okCall(caps, `notifications_post`, { id: `a/far`, title: `F`, body: null, at: far, tap: null })
    f.advance(30 * 86_400_000)
    expect(f.shown.length).toBe(0)
    f.advance(10 * 86_400_000)
    expect(f.shown.map((n) => n.title)).toEqual([`F`])
  })
})
