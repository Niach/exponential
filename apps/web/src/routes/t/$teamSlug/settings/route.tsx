import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { Fragment } from "react"
import { Separator } from "@/components/ui/separator"
import { SEGMENTED_ITEM, SEGMENTED_LIST } from "@/components/ui/tabs"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { cn } from "@/lib/utils"
import {
  SETTINGS_NAV,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SettingsLayout,
})

function SettingsLayout() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, config } = useSettingsPage(teamSlug)
  const navContext = { isCloud: Boolean(config?.isCloud) }

  return (
    <div
      className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${TAB_BAR_CLEARANCE}`}
    >
      <div>
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage {team?.name ?? `your team`} and your account
        </p>
      </div>

      <Separator />

      <div className="flex flex-col gap-6">
        {/* EXP-456: on md+ the settings nav lives in the app sidebar (the
            main nav slides out, the settings nav slides in — see
            TeamSidebar/SettingsSidebar), so this in-page nav is the mobile
            horizontally-scrollable row only (group labels hidden there). */}
        {/* EXP-616: iOS capsule segmented control look — these are route
            links, not stateful tabs, so they borrow the segmented-control
            classes (EXP-698) rather than forcing Radix Tabs semantics onto
            them. Groups flatten into one strip (their labels were already
            hidden here); the strip still scrolls horizontally, with the
            scrollbar itself hidden so the capsule edge stays clean. */}
        <nav
          className={cn(
            SEGMENTED_LIST,
            `max-w-full gap-1 self-start overflow-x-auto [scrollbar-width:none] md:hidden [&::-webkit-scrollbar]:hidden`
          )}
        >
          {SETTINGS_NAV.map((group) => {
            const items = group.items.filter((item) =>
              item.visible(permissions, navContext)
            )
            if (items.length === 0) return null
            return (
              <Fragment key={group.group}>
                {items.map((item) => (
                  <Link
                    key={item.label}
                    to={item.to}
                    params={{ teamSlug }}
                    className={SEGMENTED_ITEM}
                    activeProps={{
                      className: `border-glass-stroke-active bg-glass-active text-foreground`,
                    }}
                    inactiveProps={{
                      className: `border-transparent text-muted-foreground hover:text-foreground`,
                    }}
                  >
                    <item.icon className="h-4 w-4" />
                    {item.label}
                  </Link>
                ))}
              </Fragment>
            )
          })}
        </nav>

        <div className="min-w-0 flex-1">
          <Outlet />
        </div>
      </div>
    </div>
  )
}
