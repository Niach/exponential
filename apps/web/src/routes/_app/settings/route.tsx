import { createFileRoute, Outlet, useNavigate, useRouterState } from "@tanstack/react-router"
import { SETTINGS_NAV_TEST_IDS } from "@exp/domain-contract/settings-nav"
import { MobileDetailHeader, Separator, useIsMobile } from "@exp/ui"
import { PushedTitle } from "@/components/team/mobile-page-header"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"
import { settingsSectionLabel } from "@/routes/t/$teamSlug/settings/-shared"

// #EXP-2027 (settings-nav.json `desktopNavs.personal`): the team-less
// Personal settings. md+: the nav is the sidebar's (SettingsSidebar on
// `/settings/*`), the page the title over the section. #EXP-2109, phones: the
// index is the Settings tab, each page its own under the phone detail header.
export const Route = createFileRoute(`/_app/settings`)({
  head: () => ({ meta: [{ title: pageTitle(`Settings`) }] }),
  component: PersonalSettingsLayout,
})

function PersonalSettingsLayout() {
  return useIsMobile() ? <PhoneLayout /> : <WideLayout />
}

function WideLayout() {
  return (
    <div className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`}>
      <h1 className="text-2xl font-bold">Settings</h1>
      <Separator />
      <div className="min-w-0 flex-1">
        <Outlet />
      </div>
    </div>
  )
}

function PhoneLayout() {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const page = /^\/settings\/([^/]+)/.exec(pathname)?.[1]
  if (!page) return <Outlet />
  return (
    <div className={`flex w-full flex-col ${PAGE_END_CLEARANCE}`} data-testid={SETTINGS_NAV_TEST_IDS.page}>
      <div className="sticky top-0 z-20 pt-[env(safe-area-inset-top)]">
        <MobileDetailHeader
          title={<PushedTitle>{settingsSectionLabel(page, null) ?? `Settings`}</PushedTitle>}
          onBack={() => void navigate({ to: `/settings` })}
          backLabel="Settings"
        />
      </div>
      <div className="min-w-0 px-4 pb-4 pt-6">
        <Outlet />
      </div>
    </div>
  )
}
