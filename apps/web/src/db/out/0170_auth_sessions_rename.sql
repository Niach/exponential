ALTER TABLE "sessions" RENAME TO "auth_sessions";--> statement-breakpoint
ALTER TABLE "auth_sessions" DROP CONSTRAINT "sessions_token_unique";--> statement-breakpoint
ALTER TABLE "auth_sessions" DROP CONSTRAINT "sessions_user_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "auth_sessions" ADD CONSTRAINT "auth_sessions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "auth_sessions" ADD CONSTRAINT "auth_sessions_token_unique" UNIQUE("token");