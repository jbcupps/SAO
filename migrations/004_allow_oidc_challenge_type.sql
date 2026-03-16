ALTER TABLE webauthn_challenges
    DROP CONSTRAINT IF EXISTS webauthn_challenges_challenge_type_check;

ALTER TABLE webauthn_challenges
    ADD CONSTRAINT webauthn_challenges_challenge_type_check
    CHECK (challenge_type IN ('registration', 'authentication', 'oidc'));
