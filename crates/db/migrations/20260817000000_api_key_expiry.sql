CREATE TYPE "ApiKeyStatus" AS ENUM ('ACTIVE', 'REVOKED', 'EXPIRED');

ALTER TABLE "api_keys" ADD COLUMN "status" "ApiKeyStatus" NOT NULL DEFAULT 'ACTIVE';
ALTER TABLE "api_keys" ADD COLUMN "expiresAt" TIMESTAMP(3);

UPDATE "api_keys" SET "status" = 'REVOKED' WHERE "revokedAt" IS NOT NULL;

CREATE INDEX "api_keys_status_expiresAt_idx" ON "api_keys"("status", "expiresAt");
