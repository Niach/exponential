// The WebPlatform seam: every browser API the web capabilities touch (#VAPP-265).
// `browserPlatform()` is the real one; tests hand `webCapabilities` fakes.
// Optional members are the feature checks: an absent one = the browser lacks it.

/** The parts of a `File` the capabilities read. */
export interface FileLike {
  readonly name: string
  readonly type: string
  readonly lastModified: number
  arrayBuffer(): Promise<ArrayBuffer>
}

/** File System Access / OPFS file handle (the parts used). */
export interface FileHandleLike {
  readonly kind: `file`
  readonly name: string
  getFile(): Promise<FileLike>
  createWritable(): Promise<WritableLike>
}

export interface WritableLike {
  write(data: Blob | Uint8Array): Promise<void>
  close(): Promise<void>
}

/** An OPFS directory handle (the parts used). */
export interface DirectoryHandleLike {
  readonly kind: `directory`
  readonly name: string
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<DirectoryHandleLike>
  getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandleLike>
  values(): AsyncIterable<FileHandleLike | DirectoryHandleLike>
}

/** `showOpenFilePicker` options (File System Access). */
export interface OpenPickerOptions {
  multiple: boolean
  types?: { description?: string; accept: Record<string, string[]> }[]
  excludeAcceptAllOption?: boolean
}

export interface ShareDataLike {
  title?: string
  text?: string
  url?: string
  files?: File[]
}

/** A shown notification (the parts used). */
export interface NotificationLike {
  onclick: ((event: unknown) => void) | null
  onclose: ((event: unknown) => void) | null
  close(): void
}

export interface NotificationsLike {
  permission(): `default` | `granted` | `denied`
  requestPermission(): Promise<`default` | `granted` | `denied`>
  show(title: string, options: { body?: string; tag: string }): NotificationLike
}

export type TimerHandle = unknown

export interface WebPlatform {
  /** File System Access open picker; throws AbortError on cancel. */
  showOpenFilePicker?(options: OpenPickerOptions): Promise<FileHandleLike[]>
  /** File System Access save picker; throws AbortError on cancel. */
  showSaveFilePicker?(options: { suggestedName: string }): Promise<FileHandleLike>
  /** A hidden `<input type=file>`; resolves [] when the person cancels. */
  pickWithInput(options: { accept: string; multiple: boolean }): Promise<FileLike[]>
  /** A download through an `<a download>` blob URL. */
  download(name: string, blob: Blob): void
  /** `navigator.share` and `navigator.canShare`. */
  share?(data: ShareDataLike): Promise<void>
  canShare?(data: ShareDataLike): boolean
  /** A hidden iframe on a blob URL, `contentWindow.print()` once loaded, then removed. */
  print(blob: Blob): Promise<void>
  /** `navigator.storage.getDirectory()` (OPFS). */
  storageDirectory?(): Promise<DirectoryHandleLike>
  /** The Notification API. */
  notifications?: NotificationsLike
  now(): number
  setTimeout(fn: () => void, ms: number): TimerHandle
  clearTimeout(handle: TimerHandle): void
  /** window.focus(). */
  focus(): void
}

/** How long a blob URL (and a print iframe that never sees `afterprint`) lives. */
const BLOB_URL_TTL_MS = 60_000

/** The real browser. Call it in a page (it reads `window`, `document` and `navigator`). */
export const browserPlatform = (): WebPlatform => {
  const w = globalThis as unknown as Window &
    typeof globalThis & {
      showOpenFilePicker?: (o: unknown) => Promise<FileHandleLike[]>
      showSaveFilePicker?: (o: unknown) => Promise<FileHandleLike>
    }
  const nav = w.navigator as Navigator | undefined
  const platform: WebPlatform = {
    pickWithInput: ({ accept, multiple }) =>
      new Promise((resolve, reject) => {
        // Without a gesture the click is ignored and neither change nor cancel fires: refuse instead of hanging.
        const activation = (nav as { userActivation?: { isActive: boolean } } | undefined)?.userActivation
        if (activation && !activation.isActive) {
          reject(new Error(`the picker needs a user gesture`))
          return
        }
        const input = w.document.createElement(`input`)
        input.type = `file`
        input.multiple = multiple
        if (accept) input.accept = accept
        input.style.display = `none`
        const done = (files: FileLike[]) => {
          input.remove()
          resolve(files)
        }
        input.addEventListener(`change`, () => done(Array.from(input.files ?? [])), { once: true })
        input.addEventListener(`cancel`, () => done([]), { once: true })
        w.document.body.append(input)
        input.click()
      }),
    download: (name, blob) => {
      const url = URL.createObjectURL(blob)
      const a = w.document.createElement(`a`)
      a.href = url
      a.download = name
      a.style.display = `none`
      w.document.body.append(a)
      a.click()
      a.remove()
      w.setTimeout(() => URL.revokeObjectURL(url), BLOB_URL_TTL_MS)
    },
    print: (blob) =>
      new Promise((resolve, reject) => {
        const url = URL.createObjectURL(blob)
        const frame = w.document.createElement(`iframe`)
        frame.style.cssText = `position:fixed;width:0;height:0;border:0;opacity:0;pointer-events:none`
        let removed = false
        const remove = () => {
          if (removed) return
          removed = true
          frame.remove()
          URL.revokeObjectURL(url)
        }
        frame.addEventListener(
          `load`,
          () => {
            const win = frame.contentWindow
            if (!win) {
              remove()
              reject(new Error(`no print frame`))
              return
            }
            win.addEventListener(`afterprint`, () => w.setTimeout(remove, 0), { once: true })
            w.setTimeout(remove, BLOB_URL_TTL_MS)
            try {
              win.focus()
              win.print()
              resolve()
            } catch (error) {
              remove()
              reject(error)
            }
          },
          { once: true },
        )
        frame.src = url
        w.document.body.append(frame)
      }),
    now: () => Date.now(),
    setTimeout: (fn, ms) => w.setTimeout(fn, ms),
    clearTimeout: (handle) => w.clearTimeout(handle as number),
    focus: () => w.focus(),
  }
  if (typeof w.showOpenFilePicker === `function`) {
    const pick = w.showOpenFilePicker.bind(w)
    platform.showOpenFilePicker = (options) => pick(options)
  }
  if (typeof w.showSaveFilePicker === `function`) {
    const pick = w.showSaveFilePicker.bind(w)
    platform.showSaveFilePicker = (options) => pick(options)
  }
  if (nav && typeof nav.share === `function`) {
    platform.share = (data) => nav.share(data)
    platform.canShare = (data) => (typeof nav.canShare === `function` ? nav.canShare(data) : false)
  }
  if (nav?.storage && typeof nav.storage.getDirectory === `function`) {
    platform.storageDirectory = () => nav.storage.getDirectory() as unknown as Promise<DirectoryHandleLike>
  }
  const N = (w as { Notification?: typeof Notification }).Notification
  if (typeof N === `function`) {
    platform.notifications = {
      permission: () => N.permission,
      requestPermission: () => N.requestPermission(),
      show: (title, options) => new N(title, options) as unknown as NotificationLike,
    }
  }
  return platform
}
