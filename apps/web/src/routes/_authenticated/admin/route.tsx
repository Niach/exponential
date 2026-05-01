import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { ArrowLeft, Shield, Users, Building2 } from "lucide-react"
import { authClient } from "@/lib/auth-client"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"

export const Route = createFileRoute(`/_authenticated/admin`)({
  ssr: false,
  beforeLoad: async () => {
    const result = await authClient.getSession()
    const isAdmin = (result.data?.user as { isAdmin?: boolean } | undefined)
      ?.isAdmin
    if (!isAdmin) {
      throw redirect({ to: `/` })
    }
  },
  component: AdminLayout,
})

function AdminLayout() {
  return (
    <div className="min-h-screen flex flex-col">
      <header className="flex items-center gap-3 border-b px-4 h-12">
        <Button asChild variant="ghost" size="sm" className="-ml-2">
          <Link to="/">
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </Button>
        <Separator orientation="vertical" className="h-5" />
        <div className="flex items-center gap-2 text-sm font-medium">
          <Shield className="h-4 w-4" />
          <span>Admin</span>
        </div>
        <nav className="flex items-center gap-1 ml-4">
          <Button asChild variant="ghost" size="sm">
            <Link
              to="/admin/users"
              activeProps={{ className: `bg-accent` }}
            >
              <Users className="h-4 w-4" />
              Users
            </Link>
          </Button>
          <Button asChild variant="ghost" size="sm">
            <Link
              to="/admin/workspaces"
              activeProps={{ className: `bg-accent` }}
            >
              <Building2 className="h-4 w-4" />
              Workspaces
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
