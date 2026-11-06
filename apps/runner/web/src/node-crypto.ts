// The browser's `node:crypto` for the Engine's contract packages: `@exp/emcp`
// hashes call arguments (the audit row's `argsHash`) and exposes with Node's
// synchronous `createHash("sha256")`; WebCrypto has no synchronous digest, so
// the runner aliases `node:crypto` to this (SHA-256 only, @noble/hashes).

import { sha256 } from "@noble/hashes/sha2.js"

class Sha256 {
  private hash = sha256.create()
  update(data: string | Uint8Array): this {
    this.hash.update(typeof data === `string` ? new TextEncoder().encode(data) : data)
    return this
  }
  digest(): Uint8Array
  digest(encoding: `hex`): string
  digest(encoding?: `hex`): Uint8Array | string {
    const bytes = this.hash.digest()
    return encoding === `hex` ? Array.from(bytes, (b) => b.toString(16).padStart(2, `0`)).join(``) : bytes
  }
}

export function createHash(algorithm: string): Sha256 {
  if (algorithm !== `sha256`) throw new Error(`node:crypto in the runner: only sha256`)
  return new Sha256()
}

export const randomUUID = (): string => crypto.randomUUID()

// The rest of what the contract packages import (the Vault's sealing, the
// webhooks' HMAC): server halves the runner never calls.
const serverOnly = (name: string) => () => {
  throw new Error(`node:crypto ${name} is server-only`)
}
export const createCipheriv = serverOnly(`createCipheriv`)
export const createDecipheriv = serverOnly(`createDecipheriv`)
export const createHmac = serverOnly(`createHmac`)
export const timingSafeEqual = serverOnly(`timingSafeEqual`)
export const randomBytes = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n))
