//! `onboarding.complete` (EXP-367) — the ONE server mutation behind the
//! first-run wizard's account gate (`onboardingCompletedAt`). Idempotent on
//! the server (`apps/web/src/lib/trpc/onboarding.ts`); callers fire it
//! best-effort — a transport failure must never block the flow (the server
//! backfills the stamp on the next session read anyway).

use crate::{ApiError, TrpcClient};

/// Mark the signed-in user's onboarding complete.
pub fn complete(trpc: &TrpcClient) -> Result<(), ApiError> {
    trpc.mutation_no_input::<serde_json::Value>("onboarding.complete")
        .map(|_| ())
}
