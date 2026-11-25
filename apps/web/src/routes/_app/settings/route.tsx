import { createFileRoute, Outlet } from "@tanstack/react-router"
import { Separator } from "@exp/ui"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (settings-nav.json `desktopNavs.personal`): the team-less
// Personal settings of web md+. The nav is the sidebar's (SettingsSidebar
// on `/settings/*`), so the page is the title over the section, the same
// frame as a team's settings on md+. Phones never land here (`_app`
// forwards them into their default team's settings).
export const Route = createFileRoute(`/_app/settings`)({
  head: () => ({ meta: [{ title: pageTitle(`Settings`) }] }),
  component: PersonalSettingsLayout,
})

function PersonalSettingsLayout() {
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
