-- TOTP two-factor auth for staff: 2FA is enabled iff both columns are non-NULL.
ALTER TABLE "staff_users"
    ADD COLUMN "totpSecret" TEXT,
    ADD COLUMN "totpEnabledAt" TIMESTAMP(3);
