import { describe, expect, it } from "vitest"
import { projectPreviewMirrorSchema } from "@exp/db-schema/domain"

// The preview-target system (WebTarget/AndroidTarget/IosTarget + `targets`) was
// removed (L23). The DB mirror now carries ONLY the feedback routing target;
// run configs live in `run_configs` and are edited IDE-side. These guard the
// shrunk shape.
describe(`projectPreviewMirrorSchema`, () => {
  it(`accepts the feedback-only shape`, () => {
    const parsed = projectPreviewMirrorSchema.parse({
      feedbackProjectId: `abc-123`,
    })
    expect(parsed).toEqual({ feedbackProjectId: `abc-123` })
  })

  it(`accepts an empty object`, () => {
    expect(projectPreviewMirrorSchema.parse({})).toEqual({})
  })

  it(`drops any legacy run-target payload`, () => {
    const parsed = projectPreviewMirrorSchema.parse({
      feedbackProjectId: `abc-123`,
      targets: [{ id: `web`, name: `Web`, platform: `web` }],
    })
    expect(parsed).not.toHaveProperty(`targets`)
    expect(parsed).toEqual({ feedbackProjectId: `abc-123` })
  })
})
