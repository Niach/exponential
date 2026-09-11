-- FEED-33 compat mirror: 0111 dropped `devices.shared_team_id`, but the
-- clients in the wild (iOS 0.14.28 = the live App Store build, 0.14.29,
-- Android 0.14.31, desktop/CLI 0.14.36) still read that column off the
-- devices SHAPE to find teammates' shared servers, so it comes back as a
-- plain nullable uuid (NO foreign key) = `shared_team_ids[1]` (sorted array,
-- NULL when empty). Trigger #17 (custom/0001_triggers.sql,
-- mirror_device_shared_team_id) keeps it in step from here on; the UPDATE
-- below backfills rows written between 0110 and now. Server-written only;
-- removable when CLIENT_MIN_VERSION_IOS >= 0.14.30 AND
-- CLIENT_MIN_VERSION_ANDROID >= 0.14.32 AND CLIENT_MIN_VERSION_DESKTOP
-- (which also floors the CLI) >= 0.14.37.
ALTER TABLE "devices" ADD COLUMN "shared_team_id" uuid;--> statement-breakpoint
UPDATE "devices" SET "shared_team_id" = "shared_team_ids"[1] WHERE cardinality("shared_team_ids") > 0;
