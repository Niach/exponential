import "@dotenvx/dotenvx/config"
import {
  CreateBucketCommand,
  DeleteObjectCommand,
  GetObjectCommand,
  HeadBucketCommand,
  HeadObjectCommand,
  NoSuchKey,
  PutObjectCommand,
  S3Client,
  S3ServiceException,
  type GetObjectCommandOutput,
} from "@aws-sdk/client-s3"
import { StorageThrottledError } from "@/lib/storage/errors"

const storageBucket = process.env.S3_BUCKET || `exponential-attachments`
const storageEndpoint = process.env.S3_ENDPOINT || `http://localhost:3900`
const storageRegion = process.env.S3_REGION || `garage`

let bucketReadyPromise: Promise<void> | null = null

function createStorageClient() {
  return new S3Client({
    endpoint: storageEndpoint,
    region: storageRegion,
    forcePathStyle: true,
    credentials:
      process.env.S3_ACCESS_KEY && process.env.S3_SECRET_KEY
        ? {
            accessKeyId: process.env.S3_ACCESS_KEY,
            secretAccessKey: process.env.S3_SECRET_KEY,
          }
        : undefined,
  })
}

const storageClient = createStorageClient()

// FEED-75: the store's "slow down" answers. S3 proper says `SlowDown` (503);
// Ceph RGW (Hetzner) the same; other fronts spell it 429 or `Throttling`.
const THROTTLE_ERROR_NAMES = new Set([
  `SlowDown`,
  `Throttling`,
  `ThrottlingException`,
  `RequestLimitExceeded`,
  `TooManyRequests`,
  `TooManyRequestsException`,
  `ServiceUnavailable`,
])

function isThrottlingError(error: unknown): error is S3ServiceException {
  if (!(error instanceof S3ServiceException)) return false
  const status = error.$metadata.httpStatusCode
  return status === 503 || status === 429 || THROTTLE_ERROR_NAMES.has(error.name)
}

// The SDK already retries a throttled call three times within about a
// second, which stays inside the same throttle window (the FEED-75 uploads
// show `attempts: 3, totalRetryDelay: ~1000`). These waits come on top of
// that: ~1, 2, 4 and 8 s with jitter, ~15 s in all, long enough for a
// per-second budget to refill while the uploader's curl is still waiting.
export const STORAGE_THROTTLE_RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000]

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

/** One storage call with the throttling retries above; past them the
 *  caller gets a `StorageThrottledError` (503 + Retry-After on the routes). */
async function sendWithThrottleRetry<T>(
  send: () => Promise<T>,
  delays: readonly number[] = STORAGE_THROTTLE_RETRY_DELAYS_MS
): Promise<T> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await send()
    } catch (error) {
      if (!isThrottlingError(error)) throw error
      const base = delays[attempt]
      if (base === undefined) throw new StorageThrottledError({ cause: error })
      await sleep(base + Math.floor(Math.random() * base * 0.5))
    }
  }
}

async function createBucketIfMissing() {
  try {
    await storageClient.send(new HeadBucketCommand({ Bucket: storageBucket }))
  } catch (error) {
    if (error instanceof S3ServiceException) {
      const httpStatusCode = error.$metadata.httpStatusCode ?? 0

      // Scoped credentials (R2/Hetzner/AWS policies without HeadBucket or
      // ListBucket) commonly 403 the probe even though the bucket exists.
      // Treat it as ready and let the actual operation surface real errors.
      if (httpStatusCode === 403) {
        return
      }

      if ([404, 301, 400].includes(httpStatusCode)) {
        await storageClient.send(
          new CreateBucketCommand({ Bucket: storageBucket })
        )
        return
      }
    }

    throw error
  }
}

async function ensureBucketReady() {
  // A rejected probe (endpoint briefly unreachable at first touch) must not
  // poison every later storage call for the life of the process — evict the
  // cached rejection so the next call retries.
  bucketReadyPromise ??= createBucketIfMissing().catch((error) => {
    bucketReadyPromise = null
    throw error
  })
  await bucketReadyPromise
}

export async function uploadObject(options: {
  body: Uint8Array
  contentLength: number
  contentType: string
  key: string
}) {
  await ensureBucketReady()
  await sendWithThrottleRetry(() =>
    storageClient.send(
      new PutObjectCommand({
        Bucket: storageBucket,
        Key: options.key,
        Body: options.body,
        ContentLength: options.contentLength,
        ContentType: options.contentType,
      })
    )
  )
}

export async function getObject(
  key: string,
  options?: { range?: string }
): Promise<GetObjectCommandOutput | null> {
  await ensureBucketReady()

  try {
    return await sendWithThrottleRetry(() =>
      storageClient.send(
        new GetObjectCommand({
          Bucket: storageBucket,
          Key: key,
          // Byte-range passthrough (EXP-297): the caller forwards an already
          // validated single-range `Range` header so media players and
          // resumable downloads work against the attachment route.
          Range: options?.range,
        })
      )
    )
  } catch (error) {
    if (
      error instanceof NoSuchKey ||
      (error instanceof S3ServiceException &&
        error.$metadata.httpStatusCode === 404)
    ) {
      return null
    }

    throw error
  }
}

export interface StoredObjectHead {
  sizeBytes: number
  /** The object's stored Content-Type, or null when S3 recorded none. */
  contentType: string | null
}

/**
 * HEAD one stored object (EXP-955): the size and type S3 holds for it, or
 * null when no such key exists. This is how a signed-URL upload's row learns
 * its real size — the bytes never passed through this process.
 */
export async function headObject(
  key: string
): Promise<StoredObjectHead | null> {
  await ensureBucketReady()

  try {
    const head = await sendWithThrottleRetry(() =>
      storageClient.send(
        new HeadObjectCommand({ Bucket: storageBucket, Key: key })
      )
    )
    return {
      sizeBytes:
        typeof head.ContentLength === `number` ? head.ContentLength : 0,
      contentType: head.ContentType ?? null,
    }
  } catch (error) {
    if (
      error instanceof S3ServiceException &&
      (error.name === `NotFound` || error.$metadata.httpStatusCode === 404)
    ) {
      return null
    }

    throw error
  }
}

export async function deleteObject(key: string) {
  await ensureBucketReady()
  await sendWithThrottleRetry(() =>
    storageClient.send(
      new DeleteObjectCommand({
        Bucket: storageBucket,
        Key: key,
      })
    )
  )
}

export async function toResponseBody(
  body: GetObjectCommandOutput[`Body`]
): Promise<ReadableStream | ArrayBuffer | null> {
  if (!body) {
    return null
  }

  if (
    `transformToWebStream` in body &&
    typeof body.transformToWebStream === `function`
  ) {
    return body.transformToWebStream()
  }

  if (
    `transformToByteArray` in body &&
    typeof body.transformToByteArray === `function`
  ) {
    const bytes = await body.transformToByteArray()
    return new Uint8Array(bytes).buffer
  }

  if (`arrayBuffer` in body && typeof body.arrayBuffer === `function`) {
    return await body.arrayBuffer()
  }

  return null
}
