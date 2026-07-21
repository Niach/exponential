-- Purge rows already orphaned by pre-FK account deletions, or the
-- constraint below fails to validate on existing databases (REV2-16).
DELETE FROM "apikeys" WHERE "reference_id" NOT IN (SELECT "id" FROM "users");--> statement-breakpoint
ALTER TABLE "apikeys" ADD CONSTRAINT "apikeys_reference_id_users_id_fk" FOREIGN KEY ("reference_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;