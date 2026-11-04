import fixture from "../fixtures/team-image.json" with { type: "json" }

// #EXP-1645 (fixtures/team-image.json): THE team mark and the team image. TS
// reference; desktop domain::team_image, iOS ExpCore TeamImage.swift and
// Android domain/TeamImage.kt replay `markCases`.

export const TEAM_IMAGE_COPY = fixture.copy
export const TEAM_IMAGE_TEST_IDS = fixture.testIds
export const TEAM_IMAGE_MARK = fixture.mark
export const TEAM_IMAGE_ROW = fixture.row
export const TEAM_IMAGE_UPLOAD = fixture.upload

export type TeamMark = { kind: `image`; src: string } | { kind: `glyph` }

/** The team's image when `icon_url` holds one, else the group glyph. */
export function teamMark(iconUrl: string | null | undefined): TeamMark {
  const src = iconUrl?.trim()
  return src ? { kind: `image`, src } : { kind: `glyph` }
}

/** The upload route of a team. */
export function teamImageUploadPath(teamId: string): string {
  return TEAM_IMAGE_UPLOAD.path.replace(`{teamId}`, teamId)
}
