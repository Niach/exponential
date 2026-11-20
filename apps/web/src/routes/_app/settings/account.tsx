import { createFileRoute, useNavigate, useRouter } from "@tanstack/react-router"
import { personalSettingsPath } from "@/lib/app-paths"
import {
  AccountSettingsBody,
  loadAccountSettings,
  parseAccountSearch,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (settings-nav.json `desktopNavs.personal`): the team-less Account
// page of web md+; the provider link round trip comes back here.
export const Route = createFileRoute(`/_app/settings/account`)({
  head: () => ({
    meta: [{ title: pageTitle(`Account`, `Settings`) }],
  }),
  validateSearch: parseAccountSearch,
  loader: loadAccountSettings,
  component: AccountRoute,
})

function AccountRoute() {
  const data = Route.useLoaderData()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const router = useRouter()
  return (
    <AccountSettingsBody
      data={data}
      search={search}
      accountPath={personalSettingsPath(`account`)}
      onLinkReturnConsumed={() =>
        void navigate({ to: `/settings/account`, search: {}, replace: true })
      }
      onChanged={() => void router.invalidate()}
    />
  )
}
