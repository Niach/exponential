import { createAuthClient } from "better-auth/react"
import {
  genericOAuthClient,
  inferAdditionalFields,
} from "better-auth/client/plugins"
import {
  createCollection,
  localOnlyCollectionOptions,
} from "@tanstack/react-db"
import { z } from "zod"
import type { auth } from "@/lib/auth"

export const authClient = createAuthClient({
  baseURL:
    typeof window !== `undefined`
      ? window.location.origin
      : undefined,
  plugins: [genericOAuthClient(), inferAdditionalFields<typeof auth>()],
})

type SessionData = NonNullable<
  Awaited<ReturnType<typeof authClient.getSession>>[`data`]
>

type AuthCacheEntry = {
  id: string
  session: SessionData[`session`] | null
  user: SessionData[`user`] | null
}

const authSessionSchema = z
  .object({
    id: z.string(),
    expiresAt: z.date(),
  })
  .passthrough()

const authUserSchema = z
  .object({
    id: z.string(),
    name: z.string(),
    email: z.string(),
  })
  .passthrough()

const authStateSchema: z.ZodType<AuthCacheEntry> = z.object({
  id: z.string(),
  session: authSessionSchema.nullable(),
  user: authUserSchema.nullable(),
})

export const authStateCollection = createCollection(
  localOnlyCollectionOptions({
    id: `auth-state`,
    getKey: (item) => item.id,
    schema: authStateSchema,
  })
)
