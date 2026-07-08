// Best-effort S3 object deletion for callers that live in the server ENTRY
// graph (server-bun.ts → project-trash). Deliberately does NOT import
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
}) => { delete(key: string): Promise<void> }

export async function deleteStorageObjectsViaBun(
  keys: string[]
): Promise<void> {
  if (keys.length === 0) return
  const bun = (globalThis as { Bun?: { S3Client: BunS3ClientLike } }).Bun
  if (!bun?.S3Client) {
    console.error(
      `[bun-s3-cleanup] Bun.S3Client unavailable — skipped deleting ${keys.length} object(s)`
    )
    return
  }
  const client = new bun.S3Client({
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
