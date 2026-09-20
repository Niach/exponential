// Best-effort S3 object deletion for callers that live in the server ENTRY
// graph (server-bun.ts → board-trash). Deliberately does NOT import
// @/lib/storage: any edge from the entry chunk group into the
// @aws-sdk/client-s3 subgraph makes rollup emit the SSR chunk with
// `attachRouterServerSsrUtils` treeshaken-but-referenced, and every request
// then dies with a ReferenceError (reproduced with the v0.18.4 build; static
// and dynamic edges both trigger it). Bun ships a built-in S3 client and the
// entry only ever runs under Bun (dev goes through the nitro bridge and never
// reaches server-bun.ts), so this module needs no imports at all. Reads the
// same S3_* env contract and defaults as @/lib/storage; path-style addressing
// is Bun's default, matching the aws-sdk client's forcePathStyle.
type BunS3ClientLike = new (options: {
  endpoint: string
  region: string
  bucket: string
  accessKeyId?: string
  secretAccessKey?: string
}) => {
  delete(key: string): Promise<void>
  stat(key: string): Promise<{ size: number; type: string }>
  file(key: string): { arrayBuffer(): Promise<ArrayBuffer> }
}

type BunS3Client = InstanceType<BunS3ClientLike>

function createBunStorageClient(): BunS3Client | null {
  const bun = (globalThis as { Bun?: { S3Client: BunS3ClientLike } }).Bun
  if (!bun?.S3Client) return null
  return new bun.S3Client({
    endpoint: process.env.S3_ENDPOINT || `http://localhost:3900`,
    region: process.env.S3_REGION || `garage`,
    bucket: process.env.S3_BUCKET || `exponential-attachments`,
    ...(process.env.S3_ACCESS_KEY && process.env.S3_SECRET_KEY
      ? {
          accessKeyId: process.env.S3_ACCESS_KEY,
          secretAccessKey: process.env.S3_SECRET_KEY,
        }
      : {}),
  })
}

// Bun's S3 errors carry a `code` (`NoSuchKey`) — the only miss signal this
// client gives; anything else is a real failure the caller should see.
function isBunMissingKey(error: unknown) {
  const code = (error as { code?: unknown } | null)?.code
  return code === `NoSuchKey` || code === `NotFound`
}

/**
 * EXP-955: the attachment object probe for callers in the server ENTRY graph
 * (the size backfill sweep) — the Bun-native twin of the aws-sdk probe in
 * lib/attachments/finalize.ts. Null when this process is not Bun (the entry
 * only ever runs under Bun; tests and the dev bridge never reach it).
 */
export function bunAttachmentObjectProbe(): {
  head(key: string): Promise<{ sizeBytes: number; contentType: string | null } | null>
  read(key: string): Promise<Uint8Array | null>
  remove(key: string): Promise<void>
} | null {
  const client = createBunStorageClient()
  if (!client) return null
  return {
    head: async (key) => {
      try {
        const stat = await client.stat(key)
        return { sizeBytes: stat.size, contentType: stat.type || null }
      } catch (error) {
        if (isBunMissingKey(error)) return null
        throw error
      }
    },
    read: async (key) => {
      try {
        return new Uint8Array(await client.file(key).arrayBuffer())
      } catch (error) {
        if (isBunMissingKey(error)) return null
        throw error
      }
    },
    remove: (key) => client.delete(key),
  }
}

export async function deleteStorageObjectsViaBun(
  keys: string[]
): Promise<void> {
  if (keys.length === 0) return
  const client = createBunStorageClient()
  if (!client) {
    console.error(
      `[bun-s3-cleanup] Bun.S3Client unavailable — skipped deleting ${keys.length} object(s)`
    )
    return
  }
  await Promise.allSettled(
    keys.map(async (storageKey) => {
      try {
        await client.delete(storageKey)
      } catch (error) {
        console.error(`Failed to delete attachment object`, error)
      }
    })
  )
}
