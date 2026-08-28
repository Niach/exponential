import { createFileRoute } from "@tanstack/react-router"

// Tombstone for the retired image-only upload route (EXP-639 removed the
// handler; every current client posts to /api/issues/$issueId/files).
// Self-hosted instances usually run without CLIENT_MIN_VERSION_*, so a
// pre-/files build is not blocked at the door and would post here: without a
// route TanStack answers an HTML 404, which those clients surface as a
// mystery failure. A 410 with a one-line reason is the honest answer. No
// upload logic and nothing to authorize — the body says only what any caller
// already knows.
export const Route = createFileRoute(`/api/issues/$issueId/images`)({
  server: {
    handlers: {
      POST: () =>
        Response.json(
          {
            error: `Inline image upload moved. Update the Exponential app to attach images.`,
          },
          { status: 410 }
        ),
    },
  },
})
