import { createCipheriv, createDecipheriv, createHash, randomBytes } from "node:crypto"

// Symmetric encryption for credentials stored at rest (e.g. an agent's reported
// GitHub token). AES-256-GCM with a key derived from BETTER_AUTH_SECRET. Format:
// base64(iv):base64(tag):base64(ciphertext).
function key(): Buffer {
  const secret = process.env.BETTER_AUTH_SECRET || ``
  return createHash(`sha256`).update(secret).digest()
}

export function encryptSecret(plain: string): string {
  const iv = randomBytes(12)
  const cipher = createCipheriv(`aes-256-gcm`, key(), iv)
  const enc = Buffer.concat([cipher.update(plain, `utf8`), cipher.final()])
  const tag = cipher.getAuthTag()
  return `${iv.toString(`base64`)}:${tag.toString(`base64`)}:${enc.toString(`base64`)}`
}

export function decryptSecret(blob: string): string | null {
  try {
    const [ivB, tagB, encB] = blob.split(`:`)
    if (!ivB || !tagB || !encB) return null
    const decipher = createDecipheriv(
      `aes-256-gcm`,
      key(),
      Buffer.from(ivB, `base64`)
    )
    decipher.setAuthTag(Buffer.from(tagB, `base64`))
    return Buffer.concat([
      decipher.update(Buffer.from(encB, `base64`)),
      decipher.final(),
    ]).toString(`utf8`)
  } catch {
    return null
  }
}
