import type { ReactNode } from "react"
import { BrandHeading } from "./brand-heading"

interface AuthFormShellProps {
  children: ReactNode
  // Optional so a flow can drop the header once it settles (the device
  // page hides "Enter the code…" after the device connects).
  description?: string
  footer: ReactNode
  title?: string
}

// EXP-1176: no card. The logo sits over the title (BrandHeading), the flow's
// fields follow on the bare page gradient, the legal pair closes it.
export function AuthFormShell({
  children,
  description,
  footer,
  title,
}: AuthFormShellProps) {
  return (
    <div className="min-h-screen flex items-center justify-center px-4">
      <div className="w-full max-w-sm space-y-6">
        <BrandHeading title={title} description={description} />
        <div>
          {children}
          {footer}
        </div>
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
