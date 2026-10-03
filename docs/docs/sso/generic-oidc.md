# Generic OIDC login

Pkgly supports configurable OpenID Connect (OIDC) providers beside Google and Microsoft.
OIDC uses OAuth2 for authorization and adds a signed ID token for authentication.
The admin page groups these providers under **OAuth2 Providers → Custom OIDC providers**.
The separate **Single Sign-On → OIDC / JWT Providers (JWKS)** section validates tokens supplied through headers or cookies.
Pkgly handles the authorization-code flow, PKCE, nonce validation, and local session creation.
Provider configuration supplies the issuer, credentials, endpoints, scopes, and login label. Custom providers share the same implementation.

## Configure a provider

Use **Administration → System → OAuth2 Providers → Custom OIDC providers**.
Supply a stable provider ID, login label, issuer URL, client ID, and client secret.
Register the Pkgly callback URI with your identity provider: `https://packages.example.com/api/user/oauth2/callback`.
Enable OAuth2 and the provider, then save the settings. Changes apply without a restart.

The same configuration can come from the configuration file:

```toml
[security.oauth2]
enabled = true
redirect_base_url = "https://packages.example.com"
auto_create_users = false

[[security.oauth2.providers]]
id = "company-sso"
display_name = "Company sign-in"
enabled = true
issuer = "https://identity.example.com/team"
client_id = "pkgly"
client_secret = "<supply securely>"
scopes = ["openid", "profile", "email"]
token_endpoint_auth_method = "client_secret_basic"
id_token_signing_alg = "RS256"
```

Database settings saved through the admin API take priority over file settings at startup.
Keep secrets outside source control. Admin responses return `client_secret_configured`, without the secret.
A blank secret field in the editor retains the existing secret. The API uses an omitted or null secret for retention.
An explicit empty secret is invalid. Changing the provider ID creates a new provider and requires new credentials.

IDs accept lowercase letters, digits, hyphens, and underscores, with a maximum of 64 characters.
IDs must be unique, including disabled providers. Google, Microsoft, and existing Microsoft aliases are reserved.
The login label can change without changing the ID. Login buttons use configuration order after the built-in providers.
Use the provider ID in group-to-role mappings.
Disabled providers retain their configuration. You can disable all custom providers and keep password login available.
Removing a custom provider also removes its role mappings in the editor. The API rejects mappings for unknown custom provider IDs.

## Supported provider options

This release supports confidential clients with `client_secret_basic` or `client_secret_post` and RS256 ID tokens.
Scopes must include `openid`. Public clients, encrypted ID tokens, UserInfo, and custom identity claim names are unsupported.
Configure providers to return the required standard identity claims in the ID token.

Pkgly discovers metadata from `<issuer>/.well-known/openid-configuration` during startup and settings updates.
The discovered issuer must match the configured issuer exactly, including its path and trailing slash.
Discovery must advertise the authorization-code flow, RS256, and the configured client authentication method.
Omitted authentication methods default to `client_secret_basic`.

For providers without discovery, supply the complete endpoint block:

```toml
[security.oauth2.providers.endpoints]
authorization_url = "https://identity.example.com/authorize"
token_url = "https://identity.example.com/token"
jwks_url = "https://identity.example.com/keys"
```

All three URLs are required. They can use different paths or hosts, subject to outbound network policy.
Issuer and endpoint URLs require HTTPS without embedded credentials or fragments. The issuer cannot contain a query string.
Discovery and token requests have a ten-second timeout. Signing-key requests have a five-second timeout.
Token requests do not follow redirects. Private destinations need explicit exceptions in `security.egress`.

A failed settings update retains the previous settings and clients.
If initialization fails at startup, OAuth2 login buttons are unavailable. Password login remains available.
Use static endpoints when discovery availability is unsuitable for startup.

## Login checks and account matching

Generic callbacks validate the signature, exact issuer, client audience, expiry, subject, issue time, and saved nonce.
Only the registered RS256 algorithm is accepted. The audience can be a string or an array containing only the client ID.
A present `azp` must equal the client ID. Clock tolerance is 30 seconds.
Login state expires after five minutes. Database state survives a process restart and permits only one callback consumption.
Relevant provider or callback changes reject pending logins. Start a new login after changing configuration.

Pkgly preserves its existing account lookup:

1. Match email when `email_verified` is boolean `true`.
2. Otherwise, use the existing username lookup and normalization.
3. Deny an unmatched identity when automatic creation is disabled.
4. Apply existing creation guards when automatic creation is enabled.

A verified email match wins over a different username match.
The username comes from `preferred_username`, then `email`, then `sub`.
Pkgly normalizes that value before its username lookup. Missing or false email verification does not permit an email match.
Inactive accounts cannot obtain sessions. Keep automatic creation disabled when mapping existing accounts.

## Fork preview image

The fork publishes a Linux AMD64 preview after the PR integration tests pass.
Publication runs only for `feature/generic-oidc` from the `Matty666/pkgly` repository itself.
External PR branches cannot publish this image. CI uses its built-in GitHub token with package write access.

The image is the same `pkgly:test` image that passed the package integration tests.
CI builds it from the exact PR head and records that commit in its image metadata.
The image receives a full commit tag and a moving `generic-oidc` tag:

```sh
docker pull ghcr.io/matty666/pkgly:generic-oidc
# For repeatable QA, replace <commit> with the full PR head SHA:
docker pull ghcr.io/matty666/pkgly:<commit>
```

Use the digest from the publication log when recording deployment QA.
The preview does not update the `latest` tag or create a release.
A new package starts private. Authenticate to pull it, or make the package public in GitHub package settings.
Image publication does not change your deployment or identity provider configuration.

## Environment example: Authelia

Authelia is one possible configured provider. Pkgly contains no Authelia-specific product behavior.
Register a confidential client in Authelia with the Pkgly callback URI, chosen authentication method, RS256, and PKCE S256.
Configure Pkgly with that issuer and matching client credentials.

Authelia can return profile and email claims through UserInfo. This release requires them in the ID token.
Assign an ID-token claim policy to the Pkgly client:

```yaml
identity_providers:
  oidc:
    claims_policies:
      pkgly:
        id_token: ['preferred_username', 'email', 'email_verified', 'name']
    clients:
      - client_id: 'pkgly'
        claims_policy: 'pkgly'
        scopes: ['openid', 'profile', 'email']
        # Complete the client credentials, callback, and authentication settings.
```

For group mapping, request `groups` and add it to the ID-token policy.
Check compatibility against your deployed version and inspect claim names and types during login QA without saving raw tokens.
See [Authelia claim policies](https://www.authelia.com/integration/openid-connect/openid-connect-1.0-claims/).
