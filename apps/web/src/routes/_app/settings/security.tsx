import { createFileRoute } from "@tanstack/react-router"
import {
  loadSecuritySettings,
  SecuritySettingsBody,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: the team-less Security page of web md+.
export const Route = createFileRoute(`/_app/settings/security`)({
  head: () => ({
    meta: [{ title: pageTitle(`Security`, `Settings`) }],
  }),
  loader: loadSecuritySettings,
  component: SecurityRoute,
})

function SecurityRoute() {
  return <SecuritySettingsBody data={Route.useLoaderData()} />
}
