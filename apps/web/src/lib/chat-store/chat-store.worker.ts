/// <reference lib="webworker" />
// #EDE-115 (chat-store.json S4): the chat store's worker. OPFS's synchronous
// access handles exist only in a worker; each op opens its chat's file under a
// Web Lock (`exp-chat-store:<account>/<file>`), so every tab's worker shares
// the files safely. The wasm (scripts/build-chat-store-wasm.sh →
// public/chat-store/) loads here, at runtime, never in a bundle.
//
// Protocol: `{id, op, args}` → `{id, result}` | `{id, error}`; the first op is
// `init {account}` (result: whether this browser can keep a store at all).

import { ChatStoreCore, type ChatStoreWasm, type FileHost, type SyncFile } from "./store-core"

declare const self: DedicatedWorkerGlobalScope

const WASM_JS = `/chat-store/chat_store_wasm.js`

let core: ChatStoreCore | undefined

type SyncHandle = SyncFile & { close(): void }
type OpfsFileHandle = FileSystemFileHandle & { createSyncAccessHandle(): Promise<SyncHandle> }

async function init(account: string): Promise<boolean> {
  if (!/^(?!\.+$)[A-Za-z0-9._-]{1,255}$/.test(account)) return false
  if (!navigator.storage?.getDirectory || !navigator.locks) return false
  if (typeof FileSystemFileHandle === `undefined` || !(`createSyncAccessHandle` in FileSystemFileHandle.prototype)) return false
  const root = await navigator.storage.getDirectory()
  const dir = await (await root.getDirectoryHandle(`chat-store`, { create: true })).getDirectoryHandle(account, { create: true })
  const host: FileHost = {
    withFile: (name, body) =>
      navigator.locks.request(`exp-chat-store:${account}/${name}`, async () => {
        const handle = (await dir.getFileHandle(name, { create: true })) as OpfsFileHandle
        const file = await handle.createSyncAccessHandle()
        try {
          return body(file)
        } finally {
          file.close()
        }
      }),
    list: async () => {
      const names: string[] = []
      for await (const [name] of (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) names.push(name)
      return names
    },
  }
  const wasm = (await import(/* @vite-ignore */ new URL(WASM_JS, self.location.origin).href)) as ChatStoreWasm & {
    default: () => Promise<unknown>
  }
  await wasm.default()
  core = new ChatStoreCore(wasm, host)
  return true
}

type Request = { id: number; op: string; args: unknown[] }

/** The core's public ops (nothing else is callable from a message). */
const OPS = new Set([
  `putEvents`, `putMessages`, `openTail`, `page`, `searchChat`, `searchAll`, `meta`, `metas`, `setMeta`, `ask`, `take`, `answer`,
  `refusal`, `holders`, `mayRun`, `retryMs`, `constants`, `bucketWait`, `bucketSpend`,
])

self.onmessage = async (event: MessageEvent<Request>) => {
  const { id, op, args } = event.data
  try {
    let result: unknown
    if (op === `init`) result = await init(String(args[0]))
    else {
      if (!core) throw new Error(`chat store not ready`)
      const method = (core as unknown as Record<string, (...a: unknown[]) => unknown>)[op]
      if (!OPS.has(op) || typeof method !== `function`) throw new Error(`unknown op ${op}`)
      result = await method.apply(core, args)
    }
    self.postMessage({ id, result })
  } catch (error) {
    self.postMessage({ id, error: error instanceof Error ? error.message : String(error) })
  }
}
