import { createFileRoute, Navigate } from "@tanstack/react-router"

// #EXP-2027: bare `/settings` opens the first Personal page, Account. A
// render-time redirect, so a phone's `/settings` is forwarded by `_app`
// into its default team's settings instead.
export const Route = createFileRoute(`/_app/settings/`)({
  component: PersonalSettingsIndex,
})

function PersonalSettingsIndex() {
  return <Navigate to="/settings/account" replace />
}
