ALTER TABLE "installs" ADD COLUMN "registration_signature" varchar(86);--> statement-breakpoint
ALTER TABLE "installs" ADD COLUMN "registration_signed_at" varchar(64);--> statement-breakpoint
ALTER TABLE "session_keys" ADD COLUMN "signature" varchar(86);--> statement-breakpoint
ALTER TABLE "session_keys" ADD COLUMN "signer_endpoint_id" varchar(64);