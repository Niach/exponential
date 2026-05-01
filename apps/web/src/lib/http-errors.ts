import { TRPCError } from "@trpc/server"

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
    default:
      return 500
  }
}

export function errorToResponse(error: unknown) {
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
