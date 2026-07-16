import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import {
  SETTINGS_NAV,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: SettingsLayout,
})

function SettingsLayout() {
  const { workspaceSlug } = Route.useParams()
  const { workspace, permissions, solo, config } =
    useSettingsPage(workspaceSlug)
  const navContext = { isCloud: Boolean(config?.isCloud), solo }

  return (
    <div className="mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6">
      <div>
        <h1 className="text-2xl font-bold">
          {solo ? `Settings` : `Team Settings`}
        </h1>
        <p className="text-sm text-muted-foreground">
          {solo
            ? `Manage your projects, labels, and billing.`
            : `Manage members, invites, and labels for ${workspace?.name ?? ``}`}
        </p>
      </div>

      <Separator />

      <div className="flex flex-col gap-6 md:flex-row md:gap-8">
        {/* Grouped section nav — vertical sidebar on md+, horizontally
            scrollable row on mobile (group labels hidden there). */}
        <nav className="flex gap-4 overflow-x-auto md:w-44 md:shrink-0 md:flex-col md:gap-5 md:overflow-x-visible">
          {SETTINGS_NAV.map((group) => {
            const items = group.items.filter((item) =>
              item.visible(permissions, navContext)
            )
            if (items.length === 0) return null
            return (
              <div
                key={group.group}
                className="flex shrink-0 gap-1 md:flex-col"
              >
                <div className="hidden px-3 pb-1 text-xs font-medium text-muted-foreground md:block">
                  {group.group}
                </div>
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
                      params={{ workspaceSlug }}
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
