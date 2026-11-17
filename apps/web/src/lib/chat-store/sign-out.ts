// #EDE-120 (chat-store.json SIGN-OUT): a deliberate sign-out deletes the
// account's OPFS chat store (it holds chats decrypted), after every tab's
// worker stopped (client.ts broadcasts that). Loaded on demand at sign-out,
// never in the main bundle (the reference carries the whole fixture).
// Each file goes under its Web Lock (`exp-chat-store:<account>/<file>`), so a
// worker mid-write in another tab finishes first; a file still held is tried
// again a few times and otherwise left.

import { chatStoreOnLeave } from "@exp/domain-contract/chat-store"

/** The slice of an OPFS directory handle the wipe uses. */
export type WipeDir = {
  getDirectoryHandle(name: string): Promise<WipeDir>
  removeEntry(name: string): Promise<void>
  entries(): AsyncIterable<[string, { kind: string }]>
}

export type WipeDeps = {
  root: () => Promise<WipeDir>
  /** Runs `body` under the Web Lock `name`. */
  lock: <T>(name: string, body: () => Promise<T>) => Promise<T>
  tries?: number
  backoffMs?: number
}

const TRIES = 5
const BACKOFF_MS = 200

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Deletes `account`'s chat store files; the files that would not go. */
export async function deleteChatStoreFiles(account: string, deps: WipeDeps): Promise<string[]> {
  let dir: WipeDir
  let stores: WipeDir
  try {
    stores = await (await deps.root()).getDirectoryHandle(`chat-store`)
    dir = await stores.getDirectoryHandle(account)
  } catch {
    return []
  }
  const files: string[] = []
  for await (const [name, handle] of dir.entries()) {
    if (handle.kind === `file`) files.push(`chat-store/${account}/${name}`)
  }
  const left: string[] = []
  for (const path of chatStoreOnLeave(`sign_out`, account, files).delete) {
    const name = path.slice(path.lastIndexOf(`/`) + 1)
    const tries = deps.tries ?? TRIES
    for (let attempt = 1; ; attempt++) {
      try {
        await deps.lock(`exp-chat-store:${account}/${name}`, () => dir.removeEntry(name))
        break
      } catch (error) {
        if ((error as { name?: string }).name === `NotFoundError`) break
        if (attempt >= tries) {
          left.push(path)
          break
        }
        await sleep((deps.backoffMs ?? BACKOFF_MS) * attempt)
      }
    }
  }
  // The emptied dir goes too (a dir with anything left stays).
  if (left.length === 0) await stores.removeEntry(account).catch(() => {})
  return left
}

/** The browser's OPFS and Web Locks. */
export function browserWipeDeps(): WipeDeps {
  return {
    root: () => navigator.storage.getDirectory() as unknown as Promise<WipeDir>,
    lock: (name, body) => navigator.locks.request(name, body),
  }
}
