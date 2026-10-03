// ABOUTME: Defines security, password, SSO, OAuth, and egress configuration.
// ABOUTME: Supplies safe defaults and validation for authentication settings.
use std::{fmt, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const DEFAULT_CASBIN_MODEL: &str = include_str!("../../../resources/rbac/model.conf");
const DEFAULT_CASBIN_POLICY: &str = include_str!("../../../resources/rbac/policy.csv");

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SecuritySettings {
    pub password_rules: Option<PasswordRules>,
    pub sso: Option<SsoSettings>,
    pub oauth2: Option<OAuth2Settings>,
    #[serde(default)]
    pub egress: EgressSettings,
}
impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            password_rules: Some(PasswordRules::default()),
            sso: None,
            oauth2: None,
            egress: EgressSettings::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, ToSchema)]
#[serde(default)]
pub struct EgressSettings {
    /// Exact hostnames allowed to resolve to private addresses.
    pub allowed_hosts: Vec<String>,
    /// CIDR ranges allowed for outbound connections.
    pub allowed_cidrs: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct SsoSettings {
    /// Enable SSO support. Disabled configurations are ignored at runtime.
    pub enabled: bool,
    /// Path or URL the UI should direct to when initiating SSO.
    pub login_path: String,
    /// Text used for the SSO button in the UI.
    pub login_button_text: String,
    /// Optional external identity provider URL used to initiate the SSO flow.
    pub provider_login_url: Option<String>,
    /// Optional query parameter on the provider login URL that indicates where to redirect after authentication.
    pub provider_redirect_param: Option<String>,
    /// Automatically create a Pkgly account when the principal does not exist.
    pub auto_create_users: bool,
    /// Optional list of OIDC/JWT providers validated via JWKS.
    #[serde(default)]
    pub providers: Vec<OidcProviderConfig>,
    /// Optional list of JWT claim keys that contain role values to apply to Casbin.
    #[serde(default)]
    pub role_claims: Vec<String>,
}

impl Default for SsoSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            login_path: default_login_path(),
            login_button_text: default_login_button_text(),
            provider_login_url: None,
            provider_redirect_param: None,
            auto_create_users: false,
            providers: Vec::new(),
            role_claims: Vec::new(),
        }
    }
}

fn default_login_path() -> String {
    "/api/user/sso/login".to_string()
}

fn default_login_button_text() -> String {
    "Sign in with SSO".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenSource {
    Header {
        /// Header name that carries the bearer token.
        name: String,
        /// Optional prefix to strip (e.g., "Bearer ").
        #[serde(default)]
        prefix: Option<String>,
    },
    Cookie {
        /// Cookie name that carries the token.
        name: String,
    },
}

impl Default for TokenSource {
    fn default() -> Self {
        TokenSource::Header {
            name: "Authorization".to_string(),
            prefix: Some("Bearer ".to_string()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OidcProviderConfig {
    /// Friendly identifier for the provider (e.g., "cloudflare", "okta").
    pub name: String,
    /// Expected issuer claim.
    pub issuer: String,
    /// Expected audience/client ID.
    pub audience: String,
    /// Optional explicit JWKS endpoint; when omitted discovery will be used.
    pub jwks_url: Option<String>,
    /// Where to read the token from.
    pub token_source: TokenSource,
    /// Optional claim to use for username; defaults to preferred_username/sub.
    pub subject_claim: Option<String>,
    /// Optional claim to use for email; defaults to `email`.
    pub email_claim: Option<String>,
    /// Optional claim to use for display name; defaults to `name`.
    pub display_name_claim: Option<String>,
    /// Claims that contain role values applied to Casbin.
    #[serde(default)]
    pub role_claims: Vec<String>,
}

impl Default for OidcProviderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            issuer: String::new(),
            audience: String::new(),
            jwks_url: None,
            token_source: TokenSource::default(),
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2Settings {
    /// Enable OAuth2 login support.
    pub enabled: bool,
    /// Public route to initiate the OAuth2 login flow.
    pub login_path: String,
    /// Public callback route that identity providers redirect to.
    pub callback_path: String,
    /// Optional base URL override for redirect URLs. When empty, the application attempts to infer the base URL.
    pub redirect_base_url: Option<String>,
    /// Automatically provision users that do not already exist.
    pub auto_create_users: bool,
    /// Google OAuth2/OpenID Connect configuration.
    pub google: Option<OAuth2GoogleConfig>,
    /// Microsoft Entra ID (Azure AD) OAuth2/OpenID Connect configuration.
    pub microsoft: Option<OAuth2MicrosoftConfig>,
    /// Configurable OIDC providers, independent of provider vendors.
    pub providers: Vec<OAuth2GenericConfig>,
    /// Optional Casbin configuration for RBAC policy enforcement.
    pub casbin: Option<OAuth2CasbinConfig>,
    /// Optional mapping between provider groups/roles and Pkgly Casbin roles.
    pub group_role_mappings: Vec<OAuth2GroupRoleMapping>,
}

impl Default for OAuth2Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            login_path: "/api/user/oauth2/login".to_string(),
            callback_path: "/api/user/oauth2/callback".to_string(),
            redirect_base_url: None,
            auto_create_users: false,
            google: None,
            microsoft: None,
            providers: Vec::new(),
            casbin: Some(OAuth2CasbinConfig::default()),
            group_role_mappings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum OAuth2ProviderKind {
    Google,
    Microsoft,
    /// Stable identifier of a configured generic provider.
    Custom(String),
}

impl fmt::Display for OAuth2ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            OAuth2ProviderKind::Google => "google",
            OAuth2ProviderKind::Microsoft => "microsoft",
            OAuth2ProviderKind::Custom(id) => id.as_str(),
        };
        write!(f, "{value}")
    }
}

impl FromStr for OAuth2ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "google" => Ok(OAuth2ProviderKind::Google),
            "microsoft" | "azure" | "azure_ad" | "entra" | "entra_id" => {
                Ok(OAuth2ProviderKind::Microsoft)
            }
            other if valid_custom_provider_id(s) => {
                Ok(OAuth2ProviderKind::Custom(other.to_string()))
            }
            other => Err(format!("Invalid OAuth2 provider '{other}'")),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2GoogleConfig {
    /// OAuth2 client identifier issued by Google.
    pub client_id: String,
    /// OAuth2 client secret issued by Google.
    pub client_secret: String,
    /// Additional scopes requested during authorization.
    pub scopes: Vec<String>,
    /// Optional explicit redirect path override.
    pub redirect_path: Option<String>,
}

impl Default for OAuth2GoogleConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            redirect_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2MicrosoftConfig {
    /// OAuth2 client identifier for the Entra ID application.
    pub client_id: String,
    /// OAuth2 client secret for the Entra ID application.
    pub client_secret: String,
    /// Tenant identifier (defaults to `common` when omitted).
    pub tenant_id: Option<String>,
    /// Additional scopes requested during authorization.
    pub scopes: Vec<String>,
    /// Optional explicit redirect path override.
    pub redirect_path: Option<String>,
}

impl Default for OAuth2MicrosoftConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            tenant_id: None,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            redirect_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(default)]
pub struct OAuth2CasbinConfig {
    /// Casbin model configuration (INI format).
    pub model: String,
    /// Casbin policy rules (CSV format).
    pub policy: String,
}

impl Default for OAuth2CasbinConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_CASBIN_MODEL.trim().to_string(),
            policy: DEFAULT_CASBIN_POLICY.trim().to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct OAuth2GroupRoleMapping {
    /// Identity provider that emits the group/role claim.
    pub provider: OAuth2ProviderKind,
    /// Group or role identifier received from the provider.
    pub group: String,
    /// Pkgly Casbin roles applied when the group is present.
    pub roles: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct PasswordRules {
    pub min_length: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_number: bool,
    pub require_symbol: bool,
}
impl PasswordRules {
    pub fn validate(&self, password: &str) -> bool {
        if password.len() < self.min_length {
            return false;
        }
        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            return false;
        }
        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            return false;
        }
        if self.require_number && !password.chars().any(|c| c.is_numeric()) {
            return false;
        }
        if self.require_symbol && !password.chars().any(|c| c.is_ascii_punctuation()) {
            return false;
        }
        true
    }
}
impl Default for PasswordRules {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_number: true,
            require_symbol: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TlsConfig {
    pub private_key: PathBuf,
    pub certificate_chain: PathBuf,
}

impl TryFrom<String> for OAuth2ProviderKind {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<OAuth2ProviderKind> for String {
    fn from(value: OAuth2ProviderKind) -> Self {
        value.to_string()
    }
}

/// Confidential OIDC client configuration for an arbitrary provider.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(default, deny_unknown_fields)]
pub struct OAuth2GenericConfig {
    /// Stable ID used in login routes, state, and role mappings.
    pub id: String,
    /// Login button label.
    pub display_name: String,
    /// Whether the provider accepts logins.
    pub enabled: bool,
    /// Exact issuer identifier used for discovery and ID-token validation.
    pub issuer: String,
    /// Registered client identifier.
    pub client_id: String,
    /// Client secret, excluded from admin responses.
    pub client_secret: String,
    /// Requested scopes, including openid.
    pub scopes: Vec<String>,
    /// Optional callback override.
    pub redirect_path: Option<String>,
    /// Complete endpoint configuration, or None for issuer discovery.
    pub endpoints: Option<OAuth2GenericEndpoints>,
    /// Supported client authentication method: client_secret_basic or client_secret_post.
    pub token_endpoint_auth_method: String,
    /// Registered signature algorithm. This release supports RS256.
    pub id_token_signing_alg: String,
}

impl Default for OAuth2GenericConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            enabled: true,
            issuer: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scopes: vec!["openid".into(), "profile".into(), "email".into()],
            redirect_path: None,
            endpoints: None,
            token_endpoint_auth_method: "client_secret_basic".into(),
            id_token_signing_alg: "RS256".into(),
        }
    }
}

/// Explicit endpoints used instead of discovery metadata.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OAuth2GenericEndpoints {
    /// Browser authorization endpoint.
    pub authorization_url: String,
    /// Backend token endpoint.
    pub token_url: String,
    /// Public signing key endpoint.
    pub jwks_url: String,
}

pub(crate) fn valid_custom_provider_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && ![
            "google",
            "microsoft",
            "azure",
            "azure_ad",
            "entra",
            "entra_id",
        ]
        .contains(&id)
}

/// Validate an OIDC URL. Reject credentials, fragments, and non-HTTPS URLs.
/// Local HTTP endpoints are permitted only in unit-test builds.
pub(crate) fn validate_oidc_url(value: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(value).map_err(|_| "Invalid OIDC URL".to_string())?;
    let secure = url.scheme() == "https";
    #[cfg(test)]
    let secure = secure || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !secure
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("OIDC URLs require HTTPS without credentials or fragments".into());
    }
    Ok(url)
}

impl OAuth2GenericConfig {
    /// Validate configuration. Disabled providers still require a valid ID.
    /// Returns an error for missing credentials, unsupported options, or invalid URLs.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_custom_provider_id(&self.id) {
            return Err("Invalid or reserved custom provider ID".into());
        }
        if !self.enabled {
            return Ok(());
        }
        if self.display_name.trim().is_empty()
            || self.client_id.trim().is_empty()
            || self.client_secret.is_empty()
        {
            return Err("OIDC display name and client credentials are required".into());
        }
        let issuer = validate_oidc_url(&self.issuer)?;
        if issuer.query().is_some() {
            return Err("OIDC issuer cannot contain a query".into());
        }
        if !self.scopes.iter().any(|scope| scope == "openid") {
            return Err("OIDC scopes must include openid".into());
        }
        if !["client_secret_basic", "client_secret_post"]
            .contains(&self.token_endpoint_auth_method.as_str())
            || self.id_token_signing_alg != "RS256"
        {
            return Err("Unsupported OIDC authentication method or signing algorithm".into());
        }
        if let Some(endpoints) = &self.endpoints {
            endpoints.validate()?;
        }
        Ok(())
    }
}
impl OAuth2GenericEndpoints {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for endpoint in [&self.authorization_url, &self.token_url, &self.jwks_url] {
            validate_oidc_url(endpoint)?;
        }
        Ok(())
    }
}
impl OAuth2Settings {
    /// Validate generic provider IDs and enabled provider configurations.
    /// Returns an error for collisions or invalid provider contracts.
    pub fn validate_providers(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for provider in &self.providers {
            provider.validate()?;
            if !ids.insert(&provider.id) {
                return Err("Duplicate OIDC provider ID".into());
            }
        }
        Ok(())
    }
}

impl utoipa::PartialSchema for OAuth2ProviderKind {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <String as utoipa::PartialSchema>::schema()
    }
}
impl utoipa::ToSchema for OAuth2ProviderKind {}
