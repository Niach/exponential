import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatDate(date: Date | string): string {
  const d = typeof date === `string` ? new Date(date) : date
  return d.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` })
}
