ALTER TABLE oauth2_states ADD COLUMN nonce TEXT, ADD COLUMN callback_uri TEXT, ADD COLUMN config_fingerprint TEXT;
