ALTER TABLE "team_invites" ADD COLUMN "placeholder_user_id" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "placeholder_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "team_invites" ADD CONSTRAINT "team_invites_placeholder_user_id_users_id_fk" FOREIGN KEY ("placeholder_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_team_invites_placeholder_user" ON "team_invites" USING btree ("placeholder_user_id");