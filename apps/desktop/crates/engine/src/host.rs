//! EXP-746 — the ACP client host. EXP-1426 part 4d: the host itself (the
//! capabilities, the command loop, the session context, the kill feed, the
//! exit slot) moved into the agent runtime and is re-exported here under its
//! old path. What stays is Exponential's half: the exit that carries the
//! row's last write ([`EngineExit`]) and the host trait that receives it
//! ([`EngineHost`]), the `sessions.resolveCard` downgrade, and the steer
//! image localizer a host prompt's attachments go through
//! ([`SteerLocalizer`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use exponential_agent_runtime::engine::host::*;

/// How a run finished. `end` is the result of the row's last status write
/// (EXP-1292: `coding::end_session` for a thread session, the idle report
/// for an agent root), carried here so the CLI can apply
/// `registry::end_outcome_resolves` (EXP-641) without writing it twice.
pub type EngineExit = exponential_agent_runtime::engine::host::EngineExit<Result<(), api::error::ApiError>>;

/// The one callback the two hosts implement. Fired ONCE per run, off the
/// engine thread — the desktop marshals to the gpui foreground itself.
pub trait EngineHost: Send + Sync + 'static {
    fn on_exit(&self, exit: EngineExit);
}

/// EXP-1280: the `sessions.resolveCard` input that downgrades an approved
/// plan card's "fresh context" to what happened: `fresh` ABSENT (never
/// `false`) — the server turns a stored `fresh: true` into `false` for an
/// approval whose input does not say `true`; idempotent on a card already
/// resolved.
pub(crate) fn fresh_context_downgrade(message_id: &str) -> api::sessions::AnswerInput {
    api::sessions::AnswerInput {
        message_id: message_id.to_string(),
        decision: Some("approved".to_string()),
        ..Default::default()
    }
}

/// The worktree's steer image directory (EXP-511 embeds land here).
pub(crate) fn steer_images_dir(worktree: &Path) -> PathBuf {
    worktree.join(coding::launcher::STEER_IMAGES_DIR)
}

/// EXP-825: the run's ONE attachment localizer for the host's own prompts
/// (the seed, a local composer message): the publisher's download hook
/// (`steer::image_localizer`) and the shared restore map, so the seed
/// prompt's images (localized before any publisher exists) and every
/// steered image restore through the same map.
pub(crate) struct SteerLocalizer {
    pub(crate) hook: steer::AttachmentHook,
    pub(crate) embeds: steer::ImageEmbeds,
}

impl PromptLocalizer for SteerLocalizer {
    fn wants(&self, text: &str) -> bool {
        steer::has_attachment_embed(text)
    }

    fn localize(
        &self,
        text: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + '_>> {
        Box::pin(steer::localize_message(text, &self.hook, &self.embeds))
    }
}

/// [`SteerLocalizer`] as the core's seam.
pub(crate) fn steer_localizer(
    hook: &steer::AttachmentHook,
    embeds: &steer::ImageEmbeds,
) -> Arc<dyn PromptLocalizer> {
    Arc::new(SteerLocalizer {
        hook: Arc::clone(hook),
        embeds: embeds.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1280: the plan-mode line = the fixture's `planModeGate` (the
    /// server gate's sentence) — the core inlines it, this pins it.
    #[test]
    fn the_plan_mode_line_is_the_fixture_gate() {
        assert_eq!(plan_mode_on_line(), domain::plan_card::copy().plan_mode_gate);
        assert_eq!(PLAN_MODE_ON_LINE, domain::plan_card::copy().plan_mode_gate);
    }

    /// EXP-1280: a fresh approval that cannot clear the context (mid-turn)
    /// downgrades its card: an approval whose input leaves `fresh` ABSENT
    /// (the server's path from a stored `true` to `false`), never `false`
    /// on the wire, never another decision.
    #[test]
    fn the_fresh_context_downgrade_resolves_the_approval_without_fresh() {
        let input = fresh_context_downgrade("m-1");
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"messageId": "m-1", "decision": "approved"})
        );
        assert_eq!(input.fresh, None);
    }

    /// EXP-825: a start prompt with image embeds is localized for the agent
    /// — the agent's text carries the `Image #N: <path>` manifest, and the
    /// restore pairs land in the shared map. Without an embed the text goes
    /// through untouched and nothing is recorded (the announce/agent split
    /// itself is the core's, tested there).
    #[test]
    fn the_steer_localizer_writes_the_agent_manifest() {
        let embed = "![image](/api/attachments/11111111-2222-3333-4444-555555555555)";
        let hook: steer::AttachmentHook = Arc::new(|id, _name| {
            assert_eq!(id, "11111111-2222-3333-4444-555555555555");
            Ok(PathBuf::from("/wt/.exp-steer-images/a.png"))
        });
        let embeds = steer::ImageEmbeds::default();
        let localizer = steer_localizer(&hook, &embeds);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let text = format!("Please read the issue.\n\n## Additional instructions from the requester\n\nMatch [Image #1].\n\n{embed}\n");
        assert!(localizer.wants(&text));
        let localized = runtime.block_on(localizer.localize(text.clone()));
        assert_eq!(
            localized,
            "Please read the issue.\n\n## Additional instructions from the requester\n\nMatch [Image #1].\n\nImage #1: /wt/.exp-steer-images/a.png"
        );
        assert_eq!(
            embeds.snapshot(),
            vec![
                (
                    "Image #1: /wt/.exp-steer-images/a.png".to_string(),
                    embed.to_string()
                ),
                ("/wt/.exp-steer-images/a.png".to_string(), embed.to_string()),
            ]
        );
        // No embed: nothing to localize, no download.
        assert!(!localizer.wants("just words"));
        assert_eq!(embeds.snapshot().len(), 2);
    }
}
