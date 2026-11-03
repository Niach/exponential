import { defineCollection, z } from "astro:content"
import { docsLoader } from "@astrojs/starlight/loaders"
import { docsSchema } from "@astrojs/starlight/schema"

export const collections = {
  docs: defineCollection({
    loader: docsLoader(),
    schema: docsSchema({
      extend: z.object({
        /** Reference pages generated from the repo's docs/*.md. */
        source: z.string().optional(),
      }),
    }),
  }),
}
