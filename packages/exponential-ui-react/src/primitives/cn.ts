import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

// The class-name merger every component in this package (and every app that
// consumes it) styles through: clsx for conditionals, tailwind-merge so a
// caller's `className` beats the component's own utilities instead of racing
// it in the cascade.
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function getInitials(value: string) {
  return value
    .split(` `)
    .map((part) => part[0] ?? ``)
    .join(``)
    .toUpperCase()
    .slice(0, 2)
}
