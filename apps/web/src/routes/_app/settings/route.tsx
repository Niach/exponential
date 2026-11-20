import { createFileRoute, Outlet } from "@tanstack/react-router"

// #EXP-2027: placeholder, filled in by the page's own commit.
export const Route = createFileRoute(`/_app/settings`)({
  component: PersonalSettingsLayout,
})

function PersonalSettingsLayout() {
  return <Outlet />
}
