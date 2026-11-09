// src/start.tsx
import { createMiddleware, createStart } from "@tanstack/react-start"
import { captureException, startSpan } from "@sentry/core"

// #LOGS-2: every server function is a span (`<name>`, op `function.server`)
// and an error it throws is reported before Start serializes it to the
// client. Server-only (the compiler strips `.server()` from the client
// bundle); a no-op while diagnostics are off.
const serverFnObservability = createMiddleware({ type: `function` }).server(
  ({ next, serverFnMeta }) =>
    startSpan(
      {
        name: serverFnMeta.name,
        op: `function.server`,
        attributes: { "code.filepath": serverFnMeta.filename },
      },
      async () => {
        try {
          return await next()
        } catch (error) {
          // Thrown redirects / notFound() are control flow, not failures.
          if (error instanceof Error && !(`isNotFound` in error)) {
            captureException(error)
          }
          throw error
        }
      }
    )
)

export const startInstance = createStart(() => {
  return {
    defaultSsr: false, // or true for SSR
    functionMiddleware: [serverFnObservability],
  }
})
