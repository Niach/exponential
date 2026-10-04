import { ExponentialLogo } from "./exponential-logo"
import { cn } from "./cn"

// EXP-1176: the signed-out / first-run heading. The ONE place the mark is
// drawn big: a 56px logo over the title, nothing beside it (the wordmark is
// gone, the title names the product). `description` is for a line that
// carries STATE ("We sent a 6-digit code to …"), never for a hint that
// restates what the controls below already say.
export function BrandHeading({
  title,
  description,
  className,
}: {
  // Optional so a flow can drop its title once it settles (the device page
  // after the code is accepted); the mark always stays.
  title?: string
  description?: string
  className?: string
}) {
  return (
    <div className={cn(`flex flex-col items-center gap-3 text-center`, className)}>
      <ExponentialLogo variant="light" size={56} />
      {title && <h1 className="text-2xl font-semibold">{title}</h1>}
      {description && (
        <p className="text-sm text-muted-foreground">{description}</p>
      )}
    </div>
  )
}
