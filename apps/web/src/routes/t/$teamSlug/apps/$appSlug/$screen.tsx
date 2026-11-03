import { createFileRoute } from "@tanstack/react-router"

// VAPP-218 + VAPP-242 (apps.json `screens`, THE RUN VIEW):
// `/t/$teamSlug/apps/$appSlug/$screen?<params>` = the app RUNNING on that
// screen. The parent page (`$appSlug.tsx`) IS the run view and reads
// `$screen` + the query, so this match renders nothing of its own (a screen
// change keeps the host mounted).

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug/$screen`)({
  component: () => null,
})
