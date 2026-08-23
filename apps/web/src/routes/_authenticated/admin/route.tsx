import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import type { LinkProps } from "@tanstack/react-router"
import type React from "react"
import { useEffect, useState } from "react"
import type { LucideIcon } from "lucide-react"
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

function AdminNavLink({
  to,
  exact,
  icon: Icon,
  children,
}: {
  to: LinkProps[`to`]
  exact?: boolean
  icon: LucideIcon
  children: React.ReactNode
}) {
  return (
    <Link
      to={to}
      activeOptions={exact ? { exact: true } : undefined}
      className="inline-flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1 text-sm font-medium whitespace-nowrap transition-colors"
      activeProps={{
        className: `border-glass-stroke-active bg-glass-active text-foreground`,
      }}
      inactiveProps={{
        className: `border-transparent text-muted-foreground hover:text-foreground`,
      }}
    >
      <Icon className="h-4 w-4" />
      {children}
    </Link>
  )
}

function AdminLayout() {
  const isCloud = useIsCloud()
  return (
    <div className="min-h-screen flex flex-col">
      <header className="flex items-center gap-3 px-4 h-12">
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
        {/* EXP-616: iOS capsule segmented control (TabsList/TabsTrigger
            parity), hand-mirrored because these are route links, not stateful
            tabs. The entries outgrow narrow viewports — scroll the strip
            instead of clipping it (pills stay nowrap, so without this they
            vanish off-screen on mobile); the scrollbar itself is hidden so the
            capsule edge stays clean. */}
        <nav className="ml-4 inline-flex min-w-0 items-center gap-1 overflow-x-auto rounded-full border border-glass-stroke-section bg-glass-section p-[3px] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          <AdminNavLink to="/admin" exact icon={LayoutDashboard}>
            Overview
          </AdminNavLink>
          <AdminNavLink to="/admin/performance" icon={Activity}>
            Performance
          </AdminNavLink>
          {isCloud && (
            <AdminNavLink to="/admin/conversions" icon={TrendingUp}>
              Conversions
            </AdminNavLink>
          )}
          <AdminNavLink to="/admin/users" icon={Users}>
            Users
          </AdminNavLink>
          <AdminNavLink to="/admin/teams" icon={Building2}>
            Teams
          </AdminNavLink>
          <AdminNavLink to="/admin/email" icon={MailWarning}>
            Email
          </AdminNavLink>
        </nav>
      </header>
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  )
}
