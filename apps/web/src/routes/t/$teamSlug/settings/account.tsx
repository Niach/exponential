import { createFileRoute, useNavigate, useRouter } from "@tanstack/react-router"
import {
  AccountSettingsBody,
  loadAccountSettings,
  parseAccountSearch,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: phones keep the team-scoped Personal pages; md+ forwards this
// URL to `/settings/account` (the same body, components/settings).
export const Route = createFileRoute(`/t/$teamSlug/settings/account`)({
  head: () => ({
    meta: [{ title: pageTitle(`Account`, `Settings`) }],
  }),
  validateSearch: parseAccountSearch,
  loader: loadAccountSettings,
  component: SettingsAccount,
})

function SettingsAccount() {
  const { teamSlug } = Route.useParams()
  const data = Route.useLoaderData()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const router = useRouter()
  return (
    <AccountSettingsBody
      data={data}
      search={search}
      accountPath={`/t/${teamSlug}/settings/account`}
      onLinkReturnConsumed={() =>
        void navigate({
          to: `/t/$teamSlug/settings/account`,
          params: { teamSlug },
          search: {},
          replace: true,
        })
      }
      onChanged={() => void router.invalidate()}
    />
  )
}
