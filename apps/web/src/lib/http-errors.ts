import { TRPCError } from "@trpc/server"
import { StorageThrottledError } from "@/lib/storage/errors"
import { CLIENT_CLOSED_STATUS, logRequestFailure } from "@/lib/client-abort"

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

/** `request` = the failing request: its route, method and duration go into
 * the log line, and a client that hung up (an aborted upload's body read)
 * is a debug line + 499 instead of an error. */
export function errorToResponse(error: unknown, request?: Request) {
  // FEED-75: the object store throttled us past every retry. 503 with a
  // Retry-After is what the store said and what curl's `--retry` honours;
  // the generic 500 below hid exactly this for the upload routes.
  if (error instanceof StorageThrottledError) {
    logRequestFailure(error, request)
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

  if (logRequestFailure(error, request)) {
    return new Response(null, { status: CLIENT_CLOSED_STATUS })
  }

  return Response.json(
    {
      error: `Internal server error`,
    },
    {
      status: 500,
    }
  )
}
