import { createFileRoute } from "@tanstack/react-router"

// VAPP-218 (apps.json `screens`): `/t/$teamSlug/apps/$appSlug/$screen?<params>`
// = the app's editor on its Preview tab showing that screen. The parent page
// (`$appSlug.tsx`) IS the editor and reads `$screen` + the query, so this
// match renders nothing of its own (a screen change keeps the editor mounted).

export const Route = createFileRoute(`/t/$teamSlug/apps/$appSlug/$screen`)({
  component: () => null,
})
