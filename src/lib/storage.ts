import "@dotenvx/dotenvx/config"
import {
  CreateBucketCommand,
  DeleteObjectCommand,
  GetObjectCommand,
  HeadBucketCommand,
  NoSuchKey,
  PutObjectCommand,
  S3Client,
  S3ServiceException,
  type GetObjectCommandOutput,
} from "@aws-sdk/client-s3"

const storageBucket = process.env.MINIO_BUCKET || `exponential-attachments`
const storageEndpoint = process.env.MINIO_ENDPOINT || `http://localhost:9000`
const storageRegion = process.env.MINIO_REGION || `us-east-1`

let bucketReadyPromise: Promise<void> | null = null

function createStorageClient() {
  return new S3Client({
    endpoint: storageEndpoint,
    region: storageRegion,
    forcePathStyle: true,
    credentials:
      process.env.MINIO_ACCESS_KEY && process.env.MINIO_SECRET_KEY
        ? {
            accessKeyId: process.env.MINIO_ACCESS_KEY,
            secretAccessKey: process.env.MINIO_SECRET_KEY,
          }
        : undefined,
  })
}

const storageClient = createStorageClient()

async function createBucketIfMissing() {
  try {
    await storageClient.send(new HeadBucketCommand({ Bucket: storageBucket }))
  } catch (error) {
    if (
      error instanceof S3ServiceException &&
      [404, 301, 400].includes(error.$metadata.httpStatusCode ?? 0)
    ) {
      await storageClient.send(new CreateBucketCommand({ Bucket: storageBucket }))
      return
    }

    throw error
  }
}

async function ensureBucketReady() {
  bucketReadyPromise ??= createBucketIfMissing()
  await bucketReadyPromise
}

export async function uploadObject(options: {
  body: Uint8Array
  contentLength: number
  contentType: string
  key: string
}) {
  await ensureBucketReady()
  await storageClient.send(
    new PutObjectCommand({
      Bucket: storageBucket,
      Key: options.key,
      Body: options.body,
      ContentLength: options.contentLength,
      ContentType: options.contentType,
    })
  )
}

export async function getObject(key: string): Promise<GetObjectCommandOutput | null> {
  await ensureBucketReady()

  try {
    return await storageClient.send(
      new GetObjectCommand({
        Bucket: storageBucket,
        Key: key,
      })
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

export async function deleteObject(key: string) {
  await ensureBucketReady()
  await storageClient.send(
    new DeleteObjectCommand({
      Bucket: storageBucket,
      Key: key,
    })
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
