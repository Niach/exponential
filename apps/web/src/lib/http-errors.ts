import { TRPCError } from "@trpc/server"
import { StorageThrottledError } from "@/lib/storage/errors"

function getStatusCode(error: TRPCError) {
  switch (error.code) {
    case `BAD_REQUEST`:
      return 400
    case `UNAUTHORIZED`:
      return 401
    case `FORBIDDEN`:
      return 403
    case `NOT_FOUND`:
      return 404
    case `CONFLICT`:
      return 409
    case `PRECONDITION_FAILED`:
      return 412
    default:
      return 500
  }
}

export function errorToResponse(error: unknown) {
  // FEED-75: the object store throttled us past every retry. 503 with a
  // Retry-After is what the store said and what curl's `--retry` honours;
  // the generic 500 below hid exactly this for the upload routes.
  if (error instanceof StorageThrottledError) {
    console.error(error)
    return Response.json(
      { error: error.message },
      {
        status: 503,
        headers: { "retry-after": String(error.retryAfterSeconds) },
      }
    )
  }

  if (error instanceof TRPCError) {
    return Response.json(
      {
        error: error.message,
      },
      {
        status: getStatusCode(error),
      }
    )
  }

  console.error(error)

  return Response.json(
    {
      error: `Internal server error`,
    },
    {
      status: 500,
    }
  )
}
