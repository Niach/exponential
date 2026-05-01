import type { ReactNode } from "react"
import { ExponentialLogo } from "@/components/exponential-logo"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

interface AuthFormShellProps {
  children: ReactNode
  description: string
  footer: ReactNode
  title: string
}

export function AuthFormShell({
  children,
  description,
  footer,
  title,
}: AuthFormShellProps) {
  return (
    <div className="min-h-screen flex items-center justify-center bg-background px-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="flex items-center justify-center gap-2">
          <ExponentialLogo variant="light" size={32} />
          <span className="text-xl font-semibold">Exponential</span>
        </div>
        <Card>
          <CardHeader className="text-center">
            <CardTitle className="text-2xl">{title}</CardTitle>
            <CardDescription>{description}</CardDescription>
          </CardHeader>
          <CardContent>
            {children}
            {footer}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
