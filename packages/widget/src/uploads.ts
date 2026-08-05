// Client-side mirror of the server's reporter-picture limits (FEED-5, see
// apps/web/src/lib/widget/service.ts): up to 3 attached images, 10 MB each,
// from the server's accepted image allowlist. Imported by both the loader
// (headless submit) and the panel bundle, so it must stay dependency-free.

export const maxUploadedImages = 3

export const maxUploadedImageBytes = 10 * 1024 * 1024

export const acceptedUploadImageTypes = [
  `image/png`,
  `image/jpeg`,
  `image/webp`,
  `image/gif`,
  `image/avif`,
] as const

export function isAcceptedUploadImageType(type: string): boolean {
  return (acceptedUploadImageTypes as readonly string[]).includes(type)
}
