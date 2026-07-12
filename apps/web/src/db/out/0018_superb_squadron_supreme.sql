ALTER TABLE "fcm_tokens" DROP CONSTRAINT "fcm_tokens_token_unique";--> statement-breakpoint
ALTER TABLE "fcm_tokens" ADD CONSTRAINT "fcm_tokens_token_user_id_unique" UNIQUE("token","user_id");