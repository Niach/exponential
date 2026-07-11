import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Releases (EXP-56): workspace-level issue bundles, the 15th synced shape.
// MEMBER-ONLY: releases are internal planning artifacts, so anonymous
// feedback-board viewers get NOTHING (the empty id list yields the
// impossible-match sentinel — zero rows, no 401, same posture as
// issue-subscribers). Every column is member-safe (created_by is an opaque
// user id; the pr_* fields mirror the issues shape), so no columns allowlist.
export const Route = createFileRoute(`/api/shapes/releases`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `releases`,
        getWhere: async (userId) => {
          const workspaceIds = userId ? await getUserWorkspaceIds(userId) : []
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
