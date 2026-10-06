import { createFileRoute, useNavigate, useRouter } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { AccountOverview } from "@/components/account/account-overview"
import { DeleteAccountSection } from "@/components/account/delete-account-section"
import { PasskeysSection } from "@/components/account/passkeys-section"
import {
  SignInMethodsSection,
  type LinkReturn,
} from "@/components/account/sign-in-methods-section"
import { pageTitle } from "@/lib/page-title"

const nonEmpty = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined
// The router JSON-parses search values, so a `?link_error=1` marker arrives
// as the number 1: any present, non-empty value counts.
const present = (value: unknown): string | undefined =>
  value === undefined || value === null || value === `` || value === false
    ? undefined
    : String(value)

export const Route = createFileRoute(`/t/$teamSlug/settings/account`)({
  head: () => ({
    meta: [{ title: pageTitle(`Account`, `Settings`) }],
  }),
  // EXP-1126: where a provider link round-trip lands. `?linked=<id>` on
  // success; on failure Better Auth appends `&error=<reason>` to our
  // `?link_error=1` errorCallbackURL.
  validateSearch: (search: Record<string, unknown>): LinkSearch => ({
    linked: nonEmpty(search.linked),
    link_error: present(search.link_error),
    error: nonEmpty(search.error),
  }),
  loader: async () => {
    const [timezone, signInMethods] = await Promise.all([
      trpc.users.timezone.query(),
      trpc.users.signInMethods.query(),
    ])
    return { timezone: timezone.timezone, signInMethods }
  },
  component: SettingsAccount,
})

type LinkSearch = {
  linked?: string
  link_error?: string
  error?: string
}

// Personal section (EXP-238): identity, sign-in methods (EXP-1126: the
// changeable primary email, Google/Apple/OIDC linking, passkeys), timezone
// and account deletion. Always visible — the layout's beforeLoad already
// guarantees a session.
function SettingsAccount() {
  const { teamSlug } = Route.useParams()
  const { timezone, signInMethods } = Route.useLoaderData()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const router = useRouter()
  // EXP-1209: both bands gate the last way in on the ONE `waysIn`; a change
  // in either reloads it.
  const reloadMethods = () => void router.invalidate()

  const linkReturn: LinkReturn = {
    linked: search.linked,
    linkError: search.link_error ? (search.error ?? `unable_to_link_account`) : undefined,
  }

  return (
    <div className="space-y-6">
      <AccountOverview initialTimezone={timezone} />
      <SignInMethodsSection
        initialMethods={signInMethods}
        teamSlug={teamSlug}
        linkReturn={linkReturn}
        onLinkReturnConsumed={() =>
          void navigate({
            to: `/t/$teamSlug/settings/account`,
            params: { teamSlug },
            search: {},
            replace: true,
          })
        }
        onChanged={reloadMethods}
      />
      <PasskeysSection
        passkeyEnabled={signInMethods.passkeyEnabled}
        waysIn={signInMethods.waysIn}
        onChanged={reloadMethods}
      />
      <DeleteAccountSection />
    </div>
  )
}
