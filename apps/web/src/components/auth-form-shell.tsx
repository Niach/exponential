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
  // Optional so a flow can drop the header once it settles (the device
  // page hides "Enter the code…" after the device connects).
  description?: string
  footer: ReactNode
  title?: string
}

export function AuthFormShell({
  children,
  description,
  footer,
  title,
}: AuthFormShellProps) {
  return (
    <div className="min-h-screen flex items-center justify-center px-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="flex items-center justify-center gap-2">
          <ExponentialLogo variant="light" size={32} />
          <span className="text-xl font-semibold">Exponential</span>
        </div>
        <Card>
          {(title || description) && (
            <CardHeader className="text-center">
              {title && <CardTitle className="text-2xl">{title}</CardTitle>}
              {description && <CardDescription>{description}</CardDescription>}
            </CardHeader>
          )}
          <CardContent>
            {children}
            {footer}
          </CardContent>
        </Card>
        <p className="text-center text-xs text-muted-foreground">
          <a
            href="https://exponential.at/privacy/"
            className="underline-offset-4 hover:text-foreground hover:underline"
            target="_blank"
            rel="noreferrer"
          >
            Privacy
          </a>
          {` · `}
          <a
            href="https://exponential.at/terms/"
            className="underline-offset-4 hover:text-foreground hover:underline"
            target="_blank"
            rel="noreferrer"
          >
            Terms
          </a>
        </p>
      </div>
    </div>
  )
}
