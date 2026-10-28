ALTER TABLE "user_voice_settings" ALTER COLUMN "ptt_shortcut" SET DEFAULT '';--> statement-breakpoint
-- S-3: push to talk = a bound key; a member who had it off has no key.
UPDATE "user_voice_settings" SET "ptt_shortcut" = '' WHERE "push_to_talk" = false;
