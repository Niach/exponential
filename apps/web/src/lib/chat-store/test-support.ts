// #EDE-115: test support for the chat store: memory files shaped like OPFS
// sync access handles (one map = one browser profile's OPFS) and the real
// wasm (scripts/build-chat-store-wasm.sh → public/chat-store/, or
// CHAT_STORE_WASM=<dir>); without a build the wasm suites skip, like
// eAgent's wasm replay (the PR gate builds no wasm).

import { existsSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import type { ChatStoreWasm, FileHost, SyncFile } from "./store-core"

const dir = resolve(process.env.CHAT_STORE_WASM ?? join(__dirname, `../../../public/chat-store`))
const glue = join(dir, `chat_store_wasm.js`)

export const wasmBuilt = existsSync(glue)

let loaded: Promise<ChatStoreWasm> | undefined

export function loadTestWasm(): Promise<ChatStoreWasm> {
  loaded ??= (async () => {
    const wasm = (await import(/* @vite-ignore */ glue)) as ChatStoreWasm & { initSync: (input: { module: Uint8Array }) => unknown }
    wasm.initSync({ module: readFileSync(join(dir, `chat_store_wasm_bg.wasm`)) })
    return wasm
  })()
  return loaded
}

class MemoryFile implements SyncFile {
  constructor(public bytes: Uint8Array = new Uint8Array(0)) {}
  getSize() {
    return this.bytes.length
  }
  read(buffer: Uint8Array, { at }: { at: number }) {
    const part = this.bytes.subarray(at, at + buffer.length)
    buffer.set(part)
    return part.length
  }
  write(buffer: Uint8Array, { at }: { at: number }) {
    const size = Math.max(this.bytes.length, at + buffer.length)
    const next = new Uint8Array(size)
    next.set(this.bytes)
    next.set(buffer, at)
    this.bytes = next
    return buffer.length
  }
  truncate(size: number) {
    this.bytes = this.bytes.slice(0, size)
  }
  flush() {}
}

/** One profile's OPFS (shared by every "tab" made over it). */
export function memoryOpfs() {
  const files = new Map<string, MemoryFile>()
  let chain: Promise<unknown> = Promise.resolve()
  const host: FileHost = {
    withFile: (name, body) => {
      // The Web Lock: one op on the profile's files at a time.
      const run = chain.then(() => {
        let file = files.get(name)
        if (!file) {
          file = new MemoryFile()
          files.set(name, file)
        }
        return body(file)
      })
      chain = run.catch(() => {})
      return run
    },
    list: async () => [...files.keys()],
  }
  return { host, files }
}
