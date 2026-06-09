import type { Variants } from "motion/react"

export const fadeInUp: Variants = {
  hidden: { opacity: 0, y: 20 },
  visible: { opacity: 1, y: 0 },
}

export const staggerContainer: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.12 },
  },
}

export const cardReveal: Variants = {
  hidden: { opacity: 0, y: 24 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.5, ease: `easeOut` },
  },
}

export const heroStagger: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.1, delayChildren: 0.1 },
  },
}

export const heroChild: Variants = {
  hidden: { opacity: 0, y: 16 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.5, ease: `easeOut` },
  },
}

export const sectionReveal = {
  initial: { opacity: 0, y: 20 },
  whileInView: { opacity: 1, y: 0 },
  viewport: { once: true, amount: 0.15 },
  transition: { duration: 0.55, ease: `easeOut` },
} as const

export const viewportOnce = { once: true, amount: 0.2 } as const

/* Terminal lines appear — they never float. */
export const termLine: Variants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.12 } },
}

export const glowIn: Variants = {
  hidden: { opacity: 0, filter: `brightness(1.6)` },
  visible: {
    opacity: 1,
    filter: `brightness(1)`,
    transition: { duration: 0.5, ease: `easeOut` },
  },
}

export const slideReveal: Variants = {
  hidden: { clipPath: `inset(0 100% 0 0)` },
  visible: {
    clipPath: `inset(0 0% 0 0)`,
    transition: { duration: 0.7, ease: `easeOut` },
  },
}

export const timelineStep: Variants = {
  hidden: { opacity: 0, y: 16, scale: 0.97 },
  visible: {
    opacity: 1,
    y: 0,
    scale: 1,
    transition: { duration: 0.4, ease: `easeOut` },
  },
}
