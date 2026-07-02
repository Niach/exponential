import { createFileRoute, redirect } from "@tanstack/react-router"

type FeedbackSearch = {
  description?: string
  source?: string
  title?: string
}

export const Route = createFileRoute(`/_authenticated/feedback`)({
  validateSearch: (search: Record<string, unknown>): FeedbackSearch => ({
    title: typeof search.title === `string` ? search.title : undefined,
    description:
      typeof search.description === `string` ? search.description : undefined,
    source: typeof search.source === `string` ? search.source : undefined,
  }),
  beforeLoad: ({ search }) => {
    // The public workspace + its single Exponential project are seeded with
    // these slugs by bootstrap-cloud.ts. Land users there with the create
    // dialog prefilled.
    const description =
      [search.description, search.source && `Sent from ${search.source}`]
        .filter(Boolean)
        .join(`\n\n`) || undefined

    throw redirect({
      to: `/w/$workspaceSlug/projects/$projectSlug`,
      params: { workspaceSlug: `feedback`, projectSlug: `exponential` },
      search: {
        new: 1 as const,
        title: search.title,
        description,
      },
    })
  },
})
