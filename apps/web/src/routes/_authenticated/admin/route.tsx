import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { useEffect, useState } from "react"
import {
  Activity,
  ArrowLeft,
  LayoutDashboard,
  MailWarning,
  Shield,
  TrendingUp,
  Users,
  Building2,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { isAdminUser } from "@/lib/auth/app-user"
import { getRuntimeConfig } from "@/lib/runtime-config"

export const Route = createFileRoute(`/_authenticated/admin`)({
  ssr: false,
  beforeLoad: async ({ context }) => {
    if (!isAdminUser(context.user)) {
      throw redirect({ to: `/` })
    }
  },
  component: AdminLayout,
})

// Conversion tracking is cloud-only (EXP-362) — self-hosted admins never see
// the nav entry. One-shot module-cached fetch, same pattern as
// components/feedback-button.tsx.
let cachedIsCloud: boolean | undefined = undefined

function useIsCloud(): boolean {
  const [isCloud, setIsCloud] = useState<boolean>(cachedIsCloud ?? false)
  useEffect(() => {
    if (cachedIsCloud !== undefined) return
    void getRuntimeConfig()
      .then((config) => {
        cachedIsCloud = config.isCloud
        setIsCloud(config.isCloud)
      })
      .catch(() => {})
  }, [])
  return isCloud
}

function AdminLayout() {
  const isCloud = useIsCloud()
  return (
    <div className="min-h-screen flex flex-col">
      <header className="flex items-center gap-3 border-b px-4 h-12">
        <Button asChild variant="ghost" size="sm" className="-ml-2 shrink-0">
          <Link to="/">
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </Button>
        <Separator orientation="vertical" className="h-5 shrink-0" />
        <div className="flex shrink-0 items-center gap-2 text-sm font-medium">
          <Shield className="h-4 w-4" />
          <span className="hidden sm:inline">Admin</span>
        </div>
        {/* The entries outgrow narrow viewports — scroll the nav instead of
            clipping it (buttons stay nowrap, so without this they vanish
            off-screen on mobile). */}
        <nav className="ml-4 flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          <Button asChild variant="ghost" size="sm">
            <Link
              to="/admin"
              activeOptions={{ exact: true }}
              activeProps={{ className: `bg-accent` }}
            >
              <LayoutDashboard className="h-4 w-4" />
              Overview
            </Link>
          </Button>
          <Button asChild variant="ghost" size="sm">
            <Link
              to="/admin/performance"
              activeProps={{ className: `bg-accent` }}
            >
              <Activity className="h-4 w-4" />
              Performance
            </Link>
          </Button>
          {isCloud && (
            <Button asChild variant="ghost" size="sm">
              <Link
                to="/admin/conversions"
                activeProps={{ className: `bg-accent` }}
              >
                <TrendingUp className="h-4 w-4" />
                Conversions
              </Link>
            </Button>
          )}
          <Button asChild variant="ghost" size="sm">
            <Link to="/admin/users" activeProps={{ className: `bg-accent` }}>
              <Users className="h-4 w-4" />
              Users
            </Link>
          </Button>
          <Button asChild variant="ghost" size="sm">
            <Link to="/admin/teams" activeProps={{ className: `bg-accent` }}>
              <Building2 className="h-4 w-4" />
              Teams
            </Link>
          </Button>
          <Button asChild variant="ghost" size="sm">
            <Link to="/admin/email" activeProps={{ className: `bg-accent` }}>
              <MailWarning className="h-4 w-4" />
              Email
            </Link>
          </Button>
        </nav>
      </header>
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  )
}
