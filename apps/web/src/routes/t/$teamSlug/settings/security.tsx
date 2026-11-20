import { createFileRoute } from "@tanstack/react-router"
import {
  loadSecuritySettings,
  SecuritySettingsBody,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: phones keep the team-scoped page; md+ forwards this URL to
// `/settings/security` (the same body, components/settings).
export const Route = createFileRoute(`/t/$teamSlug/settings/security`)({
  head: () => ({
    meta: [{ title: pageTitle(`Security`, `Settings`) }],
  }),
  loader: loadSecuritySettings,
  component: SettingsSecurity,
})

function SettingsSecurity() {
  return <SecuritySettingsBody data={Route.useLoaderData()} />
}
