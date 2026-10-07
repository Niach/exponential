import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const sendMock = vi.fn()

vi.mock(`@dotenvx/dotenvx/config`, () => ({}))

vi.mock(`@aws-sdk/client-s3`, () => {
  class S3ServiceException extends Error {
    $metadata: { httpStatusCode?: number }

    constructor(httpStatusCode?: number) {
      super(`s3 error ${httpStatusCode}`)
      this.$metadata = { httpStatusCode }
    }
  }

  class NoSuchKey extends S3ServiceException {}

  class Command {
    constructor(public input: unknown) {}
  }

  return {
    S3Client: class S3Client {
      send = sendMock
    },
    S3ServiceException,
    NoSuchKey,
    HeadBucketCommand: class HeadBucketCommand extends Command {},
    CreateBucketCommand: class CreateBucketCommand extends Command {},
    PutObjectCommand: class PutObjectCommand extends Command {},
    GetObjectCommand: class GetObjectCommand extends Command {},
    DeleteObjectCommand: class DeleteObjectCommand extends Command {},
  }
})

async function loadStorage() {
  vi.resetModules()
  return await import(`./index`)
}

function commandName(call: unknown[]) {
  return (call[0] as object).constructor.name
}

async function makeServiceError(httpStatusCode: number) {
  const { S3ServiceException } = await import(`@aws-sdk/client-s3`)
  return new (S3ServiceException as unknown as new (
    status: number
  ) => Error)(httpStatusCode)
}

const uploadOptions = {
  body: new Uint8Array([1]),
  contentLength: 1,
  contentType: `image/png`,
  key: `k`,
}

describe(`ensureBucketReady`, () => {
  beforeEach(() => {
    sendMock.mockReset()
  })

  it(`probes the bucket once and caches success across calls`, async () => {
    sendMock.mockResolvedValue({})
    const storage = await loadStorage()

    await storage.uploadObject(uploadOptions)
    await storage.deleteObject(`k`)

    const heads = sendMock.mock.calls.filter(
      (call) => commandName(call) === `HeadBucketCommand`
    )
    expect(heads).toHaveLength(1)
  })

  it(`creates the bucket when the probe 404s`, async () => {
    const notFound = await makeServiceError(404)
    sendMock.mockImplementation((command: object) => {
      if (command.constructor.name === `HeadBucketCommand`) {
        return Promise.reject(notFound)
      }
      return Promise.resolve({})
    })
    const storage = await loadStorage()

    await storage.uploadObject(uploadOptions)

    expect(sendMock.mock.calls.map(commandName)).toEqual([
      `HeadBucketCommand`,
      `CreateBucketCommand`,
      `PutObjectCommand`,
    ])
  })

  it(`treats a 403 probe as "bucket exists" instead of failing`, async () => {
    const forbidden = await makeServiceError(403)
    sendMock.mockImplementation((command: object) => {
      if (command.constructor.name === `HeadBucketCommand`) {
        return Promise.reject(forbidden)
      }
      return Promise.resolve({})
    })
    const storage = await loadStorage()

    await storage.uploadObject(uploadOptions)

    expect(sendMock.mock.calls.map(commandName)).toEqual([
      `HeadBucketCommand`,
      `PutObjectCommand`,
    ])
  })

  it(`retries the probe after a failure instead of caching the rejection`, async () => {
    sendMock.mockRejectedValueOnce(new Error(`ECONNREFUSED`))
    const storage = await loadStorage()

    await expect(storage.uploadObject(uploadOptions)).rejects.toThrow(
      `ECONNREFUSED`
    )

    sendMock.mockResolvedValue({})
    await expect(storage.uploadObject(uploadOptions)).resolves.toBeUndefined()

    const heads = sendMock.mock.calls.filter(
      (call) => commandName(call) === `HeadBucketCommand`
    )
    expect(heads).toHaveLength(2)
  })

  it(`shares one in-flight probe between concurrent callers`, async () => {
    let resolveHead: (() => void) | null = null
    sendMock.mockImplementation((command: object) => {
      if (command.constructor.name === `HeadBucketCommand`) {
        return new Promise<void>((resolve) => {
          resolveHead = resolve
        })
      }
      return Promise.resolve({})
    })
    const storage = await loadStorage()

    const first = storage.uploadObject(uploadOptions)
    const second = storage.deleteObject(`k`)
    resolveHead!()
    await Promise.all([first, second])

    const heads = sendMock.mock.calls.filter(
      (call) => commandName(call) === `HeadBucketCommand`
    )
    expect(heads).toHaveLength(1)
  })
})

// FEED-75: Hetzner's Ceph front answers `503 SlowDown` under load and the
// SDK's own three attempts all land inside the same throttle window. The
// store layer waits it out with longer backoffs; past them, the caller gets
// a `StorageThrottledError` the routes answer as 503 + Retry-After.
describe(`throttling retries`, () => {
  beforeEach(() => {
    sendMock.mockReset()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  function puts() {
    return sendMock.mock.calls.filter(
      (call) => commandName(call) === `PutObjectCommand`
    )
  }

  it(`retries a throttled put after a backoff and succeeds`, async () => {
    const slowDown = await makeServiceError(503)
    let putAttempts = 0
    sendMock.mockImplementation((command: object) => {
      if (command.constructor.name !== `PutObjectCommand`) return Promise.resolve({})
      putAttempts += 1
      return putAttempts < 3 ? Promise.reject(slowDown) : Promise.resolve({})
    })
    const storage = await loadStorage()

    const upload = storage.uploadObject(uploadOptions)
    await vi.runAllTimersAsync()
    await expect(upload).resolves.toBeUndefined()
    expect(puts()).toHaveLength(3)
  })

  it(`gives up as a StorageThrottledError once the backoffs are spent`, async () => {
    const slowDown = await makeServiceError(503)
    sendMock.mockImplementation((command: object) =>
      command.constructor.name === `PutObjectCommand`
        ? Promise.reject(slowDown)
        : Promise.resolve({})
    )
    const storage = await loadStorage()
    const { StorageThrottledError } = await import(`./errors`)

    const upload = storage.uploadObject(uploadOptions)
    // Observe the rejection before the timers run so it never goes unhandled.
    const outcome = upload.then(
      () => `resolved`,
      (error: unknown) => error
    )
    await vi.runAllTimersAsync()
    const error = await outcome
    expect(error).toBeInstanceOf(StorageThrottledError)
    expect((error as InstanceType<typeof StorageThrottledError>).cause).toBe(slowDown)
    expect(puts()).toHaveLength(storage.STORAGE_THROTTLE_RETRY_DELAYS_MS.length + 1)
  })

  it(`does not retry a non-throttling store error`, async () => {
    const serverError = await makeServiceError(500)
    sendMock.mockImplementation((command: object) =>
      command.constructor.name === `PutObjectCommand`
        ? Promise.reject(serverError)
        : Promise.resolve({})
    )
    const storage = await loadStorage()

    const outcome = storage.uploadObject(uploadOptions).then(
      () => `resolved`,
      (error: unknown) => error
    )
    await vi.runAllTimersAsync()
    expect(await outcome).toBe(serverError)
    expect(puts()).toHaveLength(1)
  })

  it(`waits out a throttled delete the same way`, async () => {
    const slowDown = await makeServiceError(429)
    let attempts = 0
    sendMock.mockImplementation((command: object) => {
      if (command.constructor.name !== `DeleteObjectCommand`) return Promise.resolve({})
      attempts += 1
      return attempts === 1 ? Promise.reject(slowDown) : Promise.resolve({})
    })
    const storage = await loadStorage()

    const removal = storage.deleteObject(`k`)
    await vi.runAllTimersAsync()
    await expect(removal).resolves.toBeUndefined()
    expect(attempts).toBe(2)
  })
})
