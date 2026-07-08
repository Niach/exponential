ALTER TABLE "github_installations" DROP CONSTRAINT "github_installations_user_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "github_installations" DROP COLUMN "user_id";