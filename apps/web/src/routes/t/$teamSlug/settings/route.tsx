import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
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
        <nav className="flex gap-4 overflow-x-auto md:hidden">
          {SETTINGS_NAV.map((group) => {
            const items = group.items.filter((item) =>
              item.visible(permissions, navContext)
            )
            if (items.length === 0) return null
            return (
              <div key={group.group} className="flex shrink-0 gap-1">
                {items.map((item) => (
                  <Button
                    key={item.label}
                    asChild
                    variant="ghost"
                    size="sm"
                    className="justify-start"
                  >
                    <Link
                      to={item.to}
                      params={{ teamSlug }}
                      activeProps={{ className: `bg-accent` }}
                    >
                      <item.icon className="h-4 w-4" />
                      {item.label}
                    </Link>
                  </Button>
                ))}
              </div>
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
