use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};

use ahash::{HashMap, HashMapExt};

use oauth2::basic::{
    BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
    BasicTokenType,
};
use oauth2::url::Url;
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, ExtraTokenFields, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    StandardRevocableToken, StandardTokenResponse, TokenUrl,
};
use parking_lot::Mutex;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tracing::{trace, warn};

use crate::app::config::{
    OAuth2GoogleConfig, OAuth2MicrosoftConfig, OAuth2ProviderKind, OAuth2Settings,
};

mod generic;
mod rbac;
use crate::app::authentication::jwks::{JwksManager, ReqwestJwksFetcher};
pub use rbac::OAuth2Rbac;

const STATE_TTL: Duration = Duration::from_secs(300);

type OAuthClient<
    HasAuthUrl = EndpointNotSet,
    HasDeviceAuthUrl = EndpointNotSet,
    HasIntrospectionUrl = EndpointNotSet,
    HasRevocationUrl = EndpointNotSet,
    HasTokenUrl = EndpointNotSet,
> = Client<
    BasicErrorResponse,
    OAuthTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    HasAuthUrl,
    HasDeviceAuthUrl,
    HasIntrospectionUrl,
    HasRevocationUrl,
    HasTokenUrl,
>;

type OAuthTokenResponse = StandardTokenResponse<OidcTokenExtraFields, BasicTokenType>;
type ConfiguredOAuthClient =
    OAuthClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

#[derive(Debug, Error)]
pub enum OAuth2ServiceError {
    #[error("OAuth2 is not enabled")]
    Disabled,
    #[error("OAuth2 is enabled but no providers are configured")]
    MissingProviders,
    #[error("OAuth2 provider {0} is not configured")]
    ProviderNotConfigured(OAuth2ProviderKind),
    #[error("OAuth2 redirect URL is invalid: {0}")]
    InvalidRedirectUrl(String),
    #[error("OAuth2 login state is invalid or expired")]
    InvalidState,
    #[error("Failed to construct OAuth2 client: {0}")]
    ClientConstruction(String),
    #[error("OAuth2 token request failed: {0}")]
    TokenRequestFailed(String),
}

#[derive(Clone)]
pub struct OAuth2Service {
    settings: OAuth2Settings,
    providers: HashMap<OAuth2ProviderKind, OAuth2ProviderRuntime>,
    state_store: Arc<OAuthStateStore>,
    http_client: HttpClient,
    jwks: JwksManager<ReqwestJwksFetcher>,
}

impl OAuth2Service {
    /// Construct built-in clients without discovery. Use initialize for generic providers.
    /// Returns an error when enabled settings have no valid clients.
    pub fn new(settings: OAuth2Settings) -> Result<Option<Self>, OAuth2ServiceError> {
        if !settings.enabled {
            return Ok(None);
        }

        settings
            .validate_providers()
            .map_err(OAuth2ServiceError::ClientConstruction)?;
        let mut providers = HashMap::new();

        if let Some(cfg) = settings.google.as_ref() {
            if cfg.client_id.is_empty() || cfg.client_secret.is_empty() {
                warn!("Google OAuth2 provider is configured but missing client credentials");
            } else {
                let runtime = OAuth2ProviderRuntime::new_google(cfg, &settings)
                    .map_err(OAuth2ServiceError::ClientConstruction)?;
                providers.insert(OAuth2ProviderKind::Google, runtime);
            }
        }

        if let Some(cfg) = settings.microsoft.as_ref() {
            if cfg.client_id.is_empty() || cfg.client_secret.is_empty() {
                warn!("Microsoft OAuth2 provider is configured but missing client credentials");
            } else {
                let runtime = OAuth2ProviderRuntime::new_microsoft(cfg, &settings)
                    .map_err(OAuth2ServiceError::ClientConstruction)?;
                providers.insert(OAuth2ProviderKind::Microsoft, runtime);
            }
        }

        if providers.is_empty() && !settings.providers.iter().any(|cfg| cfg.enabled) {
            warn!("OAuth2 is enabled but no valid providers were configured");
            return Err(OAuth2ServiceError::MissingProviders);
        }

        Ok(Some(Self {
            settings,
            providers,
            state_store: Arc::new(OAuthStateStore::default()),
            http_client: oauth_http_client()?,
            jwks: JwksManager::new(
                ReqwestJwksFetcher::new().map_err(|_| {
                    OAuth2ServiceError::ClientConstruction("Cannot build JWKS client".into())
                })?,
                Duration::from_secs(3600),
            ),
        }))
    }

    /// Construct enabled clients and resolve generic discovery metadata.
    /// Returns an error for invalid configuration or unavailable discovery.
    pub async fn initialize(settings: OAuth2Settings) -> Result<Option<Self>, OAuth2ServiceError> {
        let Some(mut service) = Self::new(settings)? else {
            return Ok(None);
        };
        for config in service.settings.providers.iter().filter(|cfg| cfg.enabled) {
            let runtime = generic::runtime(config, &service.settings, &service.http_client)
                .await
                .map_err(OAuth2ServiceError::ClientConstruction)?;
            service.providers.insert(runtime.provider.clone(), runtime);
        }
        Ok(Some(service))
    }

    /// Return public descriptors in configured order, without credentials.
    pub fn provider_descriptors(&self) -> Vec<ProviderDescriptor> {
        let ordered = [OAuth2ProviderKind::Google, OAuth2ProviderKind::Microsoft]
            .into_iter()
            .chain(
                self.settings
                    .providers
                    .iter()
                    .map(|cfg| OAuth2ProviderKind::Custom(cfg.id.clone())),
            );
        ordered
            .filter_map(|kind| {
                self.providers.get(&kind).map(|runtime| ProviderDescriptor {
                    provider: kind.to_string(),
                    display_name: runtime.display_name.clone(),
                    login_path: format!(
                        "{}/{}",
                        self.settings.login_path.trim_end_matches('/'),
                        kind
                    ),
                    redirect_path: Some(runtime.redirect_path.clone()),
                })
            })
            .collect()
    }

    /// Verify a generic ID token with its configured issuer, keys, audience, and saved nonce.
    pub async fn verify_generic_token(
        &self,
        provider: &OAuth2ProviderKind,
        token: &str,
        nonce: &str,
    ) -> Result<serde_json::Map<String, Value>, String> {
        let runtime = self
            .provider_config(provider)
            .map_err(|_| "Provider unavailable")?;
        let config = runtime.generic.as_ref().ok_or("Not a generic provider")?;
        let header = jsonwebtoken::decode_header(token).map_err(|_| "Invalid identity token")?;
        if header.alg != jsonwebtoken::Algorithm::RS256 {
            return Err("Unsupported ID token algorithm".into());
        }
        let kid = header.kid.ok_or("ID token missing key ID")?;
        let jwks_url = runtime
            .jwks_url
            .as_deref()
            .ok_or("Provider missing JWKS URL")?;
        let key = self
            .jwks
            .decoding_key(&format!("{}|{}", config.issuer, jwks_url), jwks_url, &kid)
            .await
            .map_err(|_| "Cannot resolve identity signing key")?;
        generic::verify_claims(token, &key, config, nonce)
    }

    fn save_state(&self, state: &str, value: OAuthStateValue) {
        self.state_store.insert(state.to_owned(), value);
    }

    /// Remove the in-memory snapshot after authoritative database consumption.
    pub fn forget_state(&self, state: &str) {
        self.state_store.take(state);
    }

    /// Return the settings associated with this runtime.
    pub fn settings(&self) -> &OAuth2Settings {
        &self.settings
    }

    /// Return registered provider identifiers.
    pub fn providers(&self) -> impl Iterator<Item = OAuth2ProviderKind> + '_ {
        self.providers.keys().cloned()
    }

    pub fn export_state(&self, state: &str) -> Option<OAuthStateExport> {
        self.state_store.export(state)
    }

    fn provider_config(
        &self,
        provider: &OAuth2ProviderKind,
    ) -> Result<&OAuth2ProviderRuntime, OAuth2ServiceError> {
        self.providers
            .get(provider)
            .ok_or_else(|| OAuth2ServiceError::ProviderNotConfigured(provider.clone()))
    }

    /// Start authorization with PKCE and a nonce for generic clients.
    /// Returns an error for unknown providers or invalid callback URLs.
    pub fn begin_authorization(
        &self,
        provider: OAuth2ProviderKind,
        base_url: Option<&str>,
        redirect: Option<String>,
    ) -> Result<AuthorizationRedirect, OAuth2ServiceError> {
        let runtime = self.provider_config(&provider)?;
        let redirect_url = self
            .build_redirect_url(runtime, base_url)
            .map_err(OAuth2ServiceError::InvalidRedirectUrl)?;

        let client = runtime.client.clone().set_redirect_uri(
            RedirectUrl::new(redirect_url.to_string())
                .map_err(|err| OAuth2ServiceError::InvalidRedirectUrl(err.to_string()))?,
        );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client.authorize_url(CsrfToken::new_random);
        for scope in runtime.scopes.iter() {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let nonce = runtime
            .generic
            .as_ref()
            .map(|_| CsrfToken::new_random().secret().to_owned());
        if let Some(nonce) = &nonce {
            request = request.add_extra_param("nonce", nonce.clone());
        }
        let fingerprint = runtime
            .generic
            .as_ref()
            .map(|cfg| generic::fingerprint(cfg, redirect_url.as_str()))
            .transpose()
            .map_err(OAuth2ServiceError::ClientConstruction)?;
        let (auth_url, csrf_token) = request.set_pkce_challenge(pkce_challenge).url();

        self.save_state(
            csrf_token.secret(),
            OAuthStateValue {
                provider: provider.clone(),
                nonce,
                callback_uri: Some(redirect_url.to_string()),
                config_fingerprint: fingerprint,
                pkce_verifier,
                redirect,
                created_at: Instant::now(),
            },
        );

        Ok(AuthorizationRedirect {
            provider,
            authorization_url: auth_url,
            state: csrf_token.secret().to_string(),
        })
    }

    /// Consume local state and exchange the authorization code.
    /// Returns an error for expired state or failed token exchange.
    pub async fn exchange_code(
        &self,
        base_url: Option<&str>,
        code: AuthorizationCode,
        state: &str,
    ) -> Result<OAuth2Exchange, OAuth2ServiceError> {
        let Some(state_value) = self.state_store.take(state) else {
            return Err(OAuth2ServiceError::InvalidState);
        };
        self.exchange_code_with_export(
            base_url,
            code,
            OAuthStateExport {
                provider: state_value.provider,
                pkce_verifier: state_value.pkce_verifier.secret().clone(),
                redirect: state_value.redirect,
                nonce: state_value.nonce,
                callback_uri: state_value.callback_uri,
                config_fingerprint: state_value.config_fingerprint,
            },
        )
        .await
    }

    /// Exchange a code after the caller atomically consumes persisted state.
    /// Returns an error for configuration changes or failed token exchange.
    pub async fn exchange_code_with_export(
        &self,
        base_url: Option<&str>,
        code: AuthorizationCode,
        export: OAuthStateExport,
    ) -> Result<OAuth2Exchange, OAuth2ServiceError> {
        let OAuthStateExport {
            provider,
            pkce_verifier,
            redirect,
            nonce,
            callback_uri,
            config_fingerprint,
        } = export;
        let runtime = self.provider_config(&provider)?;
        let redirect_url = self
            .build_redirect_url(runtime, base_url)
            .map_err(OAuth2ServiceError::InvalidRedirectUrl)?;

        let saved_uri = callback_uri.as_deref().unwrap_or(redirect_url.as_str());
        validate_export(
            runtime,
            redirect_url.as_str(),
            saved_uri,
            nonce.as_deref(),
            config_fingerprint.as_deref(),
        )?;
        let client = runtime.client.clone().set_redirect_uri(
            RedirectUrl::new(saved_uri.to_string())
                .map_err(|err| OAuth2ServiceError::InvalidRedirectUrl(err.to_string()))?,
        );

        trace!(
            provider = %runtime.provider,
            redirect = %redirect_url,
            "Exchanging OAuth2 authorization code via restored state"
        );

        let pkce_verifier = PkceCodeVerifier::new(pkce_verifier);

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(&PolicyHttpClient(self.http_client.clone()))
            .await
            .map_err(|_| OAuth2ServiceError::TokenRequestFailed("Token exchange failed".into()))?;

        Ok(OAuth2Exchange {
            provider,
            token_response,
            redirect,
            nonce,
        })
    }

    fn build_redirect_url(
        &self,
        runtime: &OAuth2ProviderRuntime,
        base_url: Option<&str>,
    ) -> Result<Url, String> {
        let raw_path = runtime.redirect_path.as_str();
        if raw_path.starts_with("http://") || raw_path.starts_with("https://") {
            return Url::parse(raw_path)
                .map_err(|err| format!("Failed to parse redirect URL: {err}"));
        }

        let Some(base) = self
            .settings
            .redirect_base_url
            .as_deref()
            .or(base_url)
            .filter(|base| !base.is_empty())
        else {
            return Err("Redirect base URL could not be determined".to_string());
        };

        let trimmed_path = if raw_path.starts_with('/') {
            raw_path.to_string()
        } else {
            format!("/{raw_path}")
        };

        let base_url =
            Url::parse(base).map_err(|err| format!("Invalid redirect base URL '{base}': {err}"))?;
        base_url
            .join(&trimmed_path)
            .map_err(|err| format!("Failed to combine base URL and path: {err}"))
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizationRedirect {
    pub provider: OAuth2ProviderKind,
    pub authorization_url: Url,
    pub state: String,
}

#[derive(Debug)]
pub struct OAuth2Exchange {
    pub provider: OAuth2ProviderKind,
    pub token_response: OAuthTokenResponse,
    pub redirect: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthStateExport {
    pub provider: OAuth2ProviderKind,
    pub pkce_verifier: String,
    pub redirect: Option<String>,
    pub nonce: Option<String>,
    pub callback_uri: Option<String>,
    pub config_fingerprint: Option<String>,
}

#[derive(Default)]
struct OAuthStateStore {
    entries: Mutex<HashMap<String, OAuthStateValue>>,
}

impl OAuthStateStore {
    fn insert(&self, state: String, value: OAuthStateValue) {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.insert(state, value);
    }

    fn take(&self, state: &str) -> Option<OAuthStateValue> {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.remove(state)
    }

    fn export(&self, state: &str) -> Option<OAuthStateExport> {
        let mut entries = self.entries.lock();
        purge_expired_locked(&mut entries);
        entries.get(state).map(|value| OAuthStateExport {
            provider: value.provider.clone(),
            pkce_verifier: value.pkce_verifier.secret().to_string(),
            redirect: value.redirect.clone(),
            nonce: value.nonce.clone(),
            callback_uri: value.callback_uri.clone(),
            config_fingerprint: value.config_fingerprint.clone(),
        })
    }
}

fn purge_expired_locked(entries: &mut HashMap<String, OAuthStateValue>) {
    let now = Instant::now();
    entries.retain(|_, value| now.duration_since(value.created_at) < STATE_TTL);
}

struct OAuthStateValue {
    provider: OAuth2ProviderKind,
    pkce_verifier: PkceCodeVerifier,
    redirect: Option<String>,
    nonce: Option<String>,
    callback_uri: Option<String>,
    config_fingerprint: Option<String>,
    created_at: Instant,
}

#[derive(Clone)]
struct OAuth2ProviderRuntime {
    provider: OAuth2ProviderKind,
    client: ConfiguredOAuthClient,
    scopes: Vec<String>,
    redirect_path: String,
    display_name: String,
    generic: Option<crate::app::config::OAuth2GenericConfig>,
    jwks_url: Option<String>,
}

impl OAuth2ProviderRuntime {
    fn new_google(config: &OAuth2GoogleConfig, settings: &OAuth2Settings) -> Result<Self, String> {
        let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
            .map_err(|err| format!("Invalid Google authorization URL: {err}"))?;
        let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
            .map_err(|err| format!("Invalid Google token URL: {err}"))?;

        let client = OAuthClient::new(ClientId::new(config.client_id.clone()))
            .set_client_secret(ClientSecret::new(config.client_secret.clone()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url);

        let redirect_path = config
            .redirect_path
            .clone()
            .unwrap_or_else(|| settings.callback_path.clone());

        Ok(Self {
            provider: OAuth2ProviderKind::Google,
            display_name: "Google".into(),
            generic: None,
            jwks_url: None,
            client,
            scopes: normalize_scopes(&config.scopes),
            redirect_path,
        })
    }

    fn new_microsoft(
        config: &OAuth2MicrosoftConfig,
        settings: &OAuth2Settings,
    ) -> Result<Self, String> {
        let tenant = config.tenant_id.as_deref().unwrap_or("common");
        let auth_url = AuthUrl::new(format!(
            "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"
        ))
        .map_err(|err| format!("Invalid Microsoft authorization URL: {err}"))?;
        let token_url = TokenUrl::new(format!(
            "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"
        ))
        .map_err(|err| format!("Invalid Microsoft token URL: {err}"))?;

        let client = OAuthClient::new(ClientId::new(config.client_id.clone()))
            .set_client_secret(ClientSecret::new(config.client_secret.clone()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url);

        let redirect_path = config
            .redirect_path
            .clone()
            .unwrap_or_else(|| settings.callback_path.clone());

        Ok(Self {
            provider: OAuth2ProviderKind::Microsoft,
            display_name: "Microsoft".into(),
            generic: None,
            jwks_url: None,
            client,
            scopes: normalize_scopes(&config.scopes),
            redirect_path,
        })
    }
}

pub(crate) fn normalize_scopes(scopes: &[String]) -> Vec<String> {
    if scopes.is_empty() {
        return vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ];
    }
    let mut dedup = Vec::with_capacity(scopes.len());
    for scope in scopes {
        if scope.is_empty() {
            continue;
        }
        if !dedup.iter().any(|existing: &String| existing == scope) {
            dedup.push(scope.clone());
        }
    }
    if dedup.is_empty() {
        dedup.extend(
            ["openid", "profile", "email"]
                .into_iter()
                .map(str::to_string),
        );
    }
    dedup
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OidcTokenExtraFields {
    #[serde(rename = "id_token", skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(
        flatten,
        default,
        skip_serializing_if = "OidcTokenExtraFields::map_is_empty"
    )]
    pub additional: HashMap<String, Value>,
}

impl OidcTokenExtraFields {
    fn map_is_empty(map: &HashMap<String, Value>) -> bool {
        map.is_empty()
    }
}

impl ExtraTokenFields for OidcTokenExtraFields {}

#[cfg(test)]
mod tests;

/// Public description of an active OAuth2 client.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ProviderDescriptor {
    /// Stable provider ID.
    pub provider: String,
    /// Configured login button label.
    pub display_name: String,
    /// Authorization route in Pkgly.
    pub login_path: String,
    /// Configured callback route.
    pub redirect_path: Option<String>,
}

async fn token_request(
    client: &HttpClient,
    request: oauth2::HttpRequest,
) -> Result<oauth2::HttpResponse, io::Error> {
    use std::io;
    let request: reqwest::Request = request.try_into().map_err(io::Error::other)?;
    let response = crate::utils::upstream::execute(client, request)
        .await
        .map_err(|_| io::Error::other("OAuth2 token request failed"))?;
    let mut builder = http::Response::builder().status(response.status());
    for (name, value) in response.headers() {
        builder = builder.header(name, value);
    }
    builder
        .body(
            response
                .bytes()
                .await
                .map_err(|_| io::Error::other("Invalid token response body"))?
                .to_vec(),
        )
        .map_err(io::Error::other)
}

struct PolicyHttpClient(HttpClient);
impl<'c> oauth2::AsyncHttpClient<'c> for PolicyHttpClient {
    type Error = io::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<oauth2::HttpResponse, io::Error>> + Send + 'c>,
    >;
    fn call(&'c self, request: oauth2::HttpRequest) -> Self::Future {
        Box::pin(token_request(&self.0, request))
    }
}

fn oauth_http_client() -> Result<HttpClient, OAuth2ServiceError> {
    crate::utils::upstream::client_builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            OAuth2ServiceError::ClientConstruction("Cannot build OAuth2 HTTP client".into())
        })
}

fn validate_export(
    runtime: &OAuth2ProviderRuntime,
    current_uri: &str,
    saved_uri: &str,
    nonce: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(), OAuth2ServiceError> {
    if let Some(config) = &runtime.generic {
        let current = generic::fingerprint(config, current_uri)
            .map_err(OAuth2ServiceError::ClientConstruction)?;
        if nonce.is_none() || fingerprint != Some(current.as_str()) || saved_uri != current_uri {
            return Err(OAuth2ServiceError::InvalidState);
        }
    }
    Ok(())
}
