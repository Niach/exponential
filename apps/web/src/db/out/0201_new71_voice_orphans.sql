-- #NEW-71: `voice_device_id` has no FK. A voice device whose row is gone
-- (removed before devices.remove cleared it) reads as None: clearOnNone.
UPDATE "user_voice_settings" s
SET "voice_device_id" = NULL, "speak_replies" = false, "push_to_talk" = false, "wake_word" = false
WHERE s."voice_device_id" IS NOT NULL
	AND NOT EXISTS (
		SELECT 1 FROM "devices" d
		WHERE d."user_id" = s."user_id" AND d."device_id" = s."voice_device_id"
	);--> statement-breakpoint
-- #NEW-71: the 0193 backfill kept any existing agent; only an agent of one
-- of the member's teams is theirs to talk to (voice.update's rule).
UPDATE "user_voice_settings" s
SET "agent_id" = NULL, "speak_replies" = false, "push_to_talk" = false, "wake_word" = false
WHERE s."agent_id" IS NOT NULL
	AND NOT EXISTS (
		SELECT 1 FROM "agents" a
		JOIN "team_members" tm ON tm."team_id" = a."team_id"
		WHERE a."id" = s."agent_id" AND tm."user_id" = s."user_id"
	);
