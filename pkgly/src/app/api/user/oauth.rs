// ABOUTME: Implements OAuth2 provider discovery, login, callback, and RBAC sync.
// ABOUTME: Exchanges provider identities for local sessions and redirect responses.
use std::{io, net::SocketAddr, str::FromStr};

use crate::app::authentication::jwks::{JwksManager, ReqwestJwksFetcher};
use crate::app::config::{OidcProviderConfig, TokenSource};
use axum::{
    extract::{ConnectInfo, Extension, Path, Query, State},
    http::{
        StatusCode,
        header::{LOCATION, SET_COOKIE},
    },
    response::{IntoResponse, Response},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use chrono::{DateTime, Duration, Utc};
use nr_core::database::entities::user::{UserSafeData, UserType};
use nr_core::user::permissions::UpdatePermissions;
use oauth2::AuthorizationCode;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{error, info, instrument, warn};
use utoipa::{IntoParams, ToSchema};

use crate::{
    app::{
        Pkgly,
        authentication::oauth::{OAuth2ServiceError, OAuthStateExport},
        config::{OAuth2GroupRoleMapping, OAuth2ProviderKind, OAuth2Settings},
    },
    error::{InternalError, OtherInternalError},
    utils::{
        ResponseBuilder, api_error_response::APIErrorResponse,
        request_logging::access_log::AccessLogContext,
    },
};

use super::{
    session_cookie,
    sso::{SsoPrincipal, create_user, normalize_username, sanitize_redirect},
};

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OAuthAuthorizeQuery {
    redirect: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    redirect: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OAuthProviderDescriptor {
    pub provider: String,
    pub login_path: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OAuthProvidersResponse {
    pub providers: Vec<OAuthProviderDescriptor>,
}

#[derive(Clone, Debug, Deserialize)]
struct IdTokenClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    given_name: Option<String>,
    #[serde(default)]
    roles: Option<Vec<String>>,
    #[serde(default)]
    groups: Option<Vec<String>>,
}

const OAUTH_STATE_TTL_SECONDS: i64 = 300;

struct PersistedOAuthState {
    provider: String,
    pkce_verifier: String,
    redirect: Option<String>,
    created_at: DateTime<Utc>,
    nonce: Option<String>,
    callback_uri: Option<String>,
    config_fingerprint: Option<String>,
}

impl PersistedOAuthState {
    fn is_expired(&self) -> bool {
        let cutoff = Utc::now() - Duration::seconds(OAUTH_STATE_TTL_SECONDS);
        self.created_at < cutoff
    }

    fn into_export(self) -> Result<OAuthStateExport, OAuth2ServiceError> {
        let provider = OAuth2ProviderKind::from_str(&self.provider)
            .map_err(|_| OAuth2ServiceError::InvalidState)?;
        Ok(OAuthStateExport {
            provider,
            pkce_verifier: self.pkce_verifier,
            redirect: self.redirect,
            nonce: self.nonce,
            callback_uri: self.callback_uri,
            config_fingerprint: self.config_fingerprint,
        })
    }
}

#[utoipa::path(
    get,
    path = "/oauth2/providers",
    responses((status = 200, body = OAuthProvidersResponse)),
    tag = "user",
    security(())
)]
#[instrument(skip(site), fields(project_module = "Authentication", auth.oauth2 = true))]
pub async fn list_providers(State(site): State<Pkgly>) -> Result<Response, InternalError> {
    let Some(_settings) = site.oauth2_settings() else {
        return Ok(ResponseBuilder::not_found().body("OAuth2 login is not enabled"));
    };

    let providers = site
        .oauth2_service()
        .map(|service| {
            service
                .provider_descriptors()
                .into_iter()
                .map(|item| OAuthProviderDescriptor {
                    provider: item.provider,
                    login_path: item.login_path,
                    display_name: item.display_name,
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ResponseBuilder::ok().json(&OAuthProvidersResponse { providers }))
}

#[utoipa::path(
    get,
    path = "/oauth2/login/{provider}",
    params(
        ("provider" = String, Path, description = "OAuth2 provider identifier"),
        OAuthAuthorizeQuery
    ),
    responses(
        (status = 303, description = "Redirect to external provider"),
        (status = 404, description = "OAuth2 provider not configured")
    ),
    tag = "user",
    security(())
)]
#[instrument(
    skip(site, query),
    fields(project_module = "Authentication", auth.oauth2 = true, auth.oauth2.provider = %provider)
)]
pub async fn authorize(
    State(site): State<Pkgly>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthAuthorizeQuery>,
) -> Result<Response, InternalError> {
    let Some(service) = site.oauth2_service() else {
        return Ok(ResponseBuilder::not_found().body("OAuth2 login is not enabled"));
    };
    let provider_kind = match OAuth2ProviderKind::from_str(&provider) {
        Ok(kind) => kind,
        Err(_) => {
            return Ok(ResponseBuilder::not_found().body("Unknown OAuth2 provider"));
        }
    };

    let base_url = resolve_base_url(&site);

    let auth_redirect = match service.begin_authorization(
        provider_kind,
        base_url.as_deref(),
        query.redirect.clone(),
    ) {
        Ok(redirect) => redirect,
        Err(err) => {
            warn!(%err, "Failed to start OAuth2 authorization");
            return Ok(oauth_service_error_response(err));
        }
    };

    let snapshot = service.export_state(&auth_redirect.state).ok_or_else(|| {
        InternalError::from(OtherInternalError::new(io::Error::new(
            io::ErrorKind::Other,
            "OAuth2 state initialization failed",
        )))
    })?;
    persist_oauth_state(&site, &auth_redirect.state, &snapshot).await?;

    let response = ResponseBuilder::default()
        .status(StatusCode::SEE_OTHER)
        .header(LOCATION, auth_redirect.authorization_url.as_str())
        .empty();

    Ok(response)
}

#[utoipa::path(
    get,
    path = "/oauth2/callback",
    params(OAuthCallbackQuery),
    responses(
        (status = 303, description = "OAuth2 login completed"),
        (status = 400, description = "OAuth2 provider returned an error"),
        (status = 404, description = "OAuth2 login disabled")
    ),
    tag = "user",
    security(())
)]
#[instrument(
    skip(site, query, user_agent),
    fields(project_module = "Authentication", auth.oauth2 = true)
)]
pub async fn callback(
    State(site): State<Pkgly>,
    Extension(access_log): Extension<AccessLogContext>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    user_agent: Option<TypedHeader<UserAgent>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, InternalError> {
    if let Some(error) = query.error.as_ref() {
        let api_error: APIErrorResponse<String, ()> = APIErrorResponse {
            message: error.clone().into(),
            details: query.error_description.clone(),
            error: None,
        };
        return Ok(ResponseBuilder::bad_request().json(&api_error));
    }

    let code = match query.code.as_ref() {
        Some(code) if !code.is_empty() => code,
        _ => {
            let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
                message: "Missing authorization code".into(),
                details: None,
                error: None,
            };
            return Ok(ResponseBuilder::bad_request().json(&api_error));
        }
    };
    let state = match query.state.as_ref() {
        Some(state) if !state.is_empty() => state,
        _ => {
            let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
                message: "Missing state parameter".into(),
                details: None,
                error: None,
            };
            return Ok(ResponseBuilder::bad_request().json(&api_error));
        }
    };

    let Some(service) = site.oauth2_service() else {
        return Ok(ResponseBuilder::not_found().body("OAuth2 login is not enabled"));
    };
    let base_url = resolve_base_url(&site);
    let Some(persisted) = load_persisted_oauth_state(&site, state).await? else {
        return Ok(oauth_service_error_response(
            OAuth2ServiceError::InvalidState,
        ));
    };
    service.forget_state(state);
    if persisted.is_expired() {
        return Ok(oauth_service_error_response(
            OAuth2ServiceError::InvalidState,
        ));
    }
    let export = match persisted.into_export() {
        Ok(export) => export,
        Err(err) => return Ok(oauth_service_error_response(err)),
    };
    let exchange = match service
        .exchange_code_with_export(
            base_url.as_deref(),
            AuthorizationCode::new(code.clone()),
            export,
        )
        .await
    {
        Ok(exchange) => exchange,
        Err(err) => {
            warn!(%err, "OAuth2 code exchange failed");
            return Ok(oauth_service_error_response(err));
        }
    };

    let oauth_settings = service.settings().clone();

    let id_token = match exchange.token_response.extra_fields().id_token.as_ref() {
        Some(token) => token,
        None => {
            warn!("OAuth2 provider did not return an id_token");
            let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
                message: "OAuth2 provider did not return an id_token".into(),
                details: None,
                error: None,
            };
            return Ok(ResponseBuilder::internal_server_error().json(&api_error));
        }
    };

    // Create a JWKS manager for OIDC token verification
    let fetcher = match ReqwestJwksFetcher::new() {
        Ok(fetcher) => fetcher,
        Err(err) => {
            error!(%err, "Failed to create JWKS fetcher");
            let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
                message: "Unable to verify identity token".into(),
                details: None,
                error: None,
            };
            return Ok(ResponseBuilder::internal_server_error().json(&api_error));
        }
    };

    let jwks_manager = JwksManager::new(fetcher, std::time::Duration::from_secs(3600));

    let verified = match &exchange.provider {
        OAuth2ProviderKind::Custom(_) => match exchange.nonce.as_deref() {
            Some(nonce) => {
                service
                    .verify_generic_token(&exchange.provider, id_token, nonce)
                    .await
            }
            None => Err("Missing saved OIDC nonce".into()),
        },
        _ => {
            verify_builtin_token(&exchange.provider, &oauth_settings, id_token, &jwks_manager).await
        }
    };
    let claims_map = match verified {
        Ok(claims) => claims,
        Err(_) => return Ok(ResponseBuilder::unauthorized().body("Invalid identity token")),
    };

    // Extract the claims we need from the verified token
    let claims =
        match serde_json::from_value::<IdTokenClaims>(serde_json::Value::Object(claims_map)) {
            Ok(claims) => claims,
            Err(err) => {
                error!(%err, "Failed to parse verified id_token claims");
                let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
                    message: "Unable to parse identity token claims".into(),
                    details: None,
                    error: None,
                };
                return Ok(ResponseBuilder::internal_server_error().json(&api_error));
            }
        };

    let claim_groups = extract_roles(exchange.provider.clone(), &claims);
    let mapped_roles = map_roles_from_claims(
        exchange.provider.clone(),
        &claim_groups,
        &oauth_settings.group_role_mappings,
    );
    let has_mapped_roles = !mapped_roles.is_empty();
    let principal = build_principal(&claims);
    let subject_identifier = principal.email.as_deref().unwrap_or(&principal.username);

    let rbac = site.oauth2_rbac();
    let mut existing_roles: Vec<String> = Vec::new();
    if let Some(ref rbac_engine) = rbac {
        match rbac_engine.roles_for_user(subject_identifier).await {
            Ok(roles) => existing_roles = roles,
            Err(err) => {
                warn!(
                    %err,
                    provider = %exchange.provider,
                    subject = subject_identifier,
                    "Failed to load existing OAuth2 RBAC roles"
                );
            }
        }
    }
    let has_existing_roles = !existing_roles.is_empty();

    if rbac.is_some() && !has_mapped_roles && !has_existing_roles {
        warn!(
            provider = %exchange.provider,
            subject = subject_identifier,
            "OAuth2 login denied: no roles mapped for subject"
        );
        return Ok(oauth_denied_redirect("no_roles"));
    }

    let rbac_enabled = rbac.is_some();

    let mut user = match resolve_oauth_user(&site, &oauth_settings, &principal).await {
        Ok(user) => user,
        Err(response) => return Ok(response),
    };

    if !user.active {
        warn!(user_id = user.id, "Inactive user attempted OAuth2 login");
        return Ok(oauth_denied_redirect("inactive"));
    }

    let email = user
        .email
        .as_ref()
        .map(|e| e.to_string())
        .unwrap_or_else(|| user.username.to_string());
    if existing_roles.is_empty() && subject_identifier != email {
        if let Some(ref rbac_engine) = rbac {
            match rbac_engine.roles_for_user(&email).await {
                Ok(roles) => existing_roles = roles,
                Err(err) => {
                    warn!(
                        %err,
                        provider = %exchange.provider,
                        subject = %email,
                        "Failed to load existing OAuth2 RBAC roles for resolved email"
                    );
                }
            }
        }
    }

    if has_mapped_roles {
        if let Err(err) = site.apply_oauth_roles(&email, &mapped_roles).await {
            warn!(%err, user_id = user.id, "Failed to apply OAuth2 RBAC roles");
        }
    }

    let effective_roles = if has_mapped_roles {
        mapped_roles.clone()
    } else {
        existing_roles.clone()
    };

    let admin_role = effective_roles
        .iter()
        .any(|role| role.eq_ignore_ascii_case("admin"));
    let user_manager_role = effective_roles
        .iter()
        .any(|role| role.eq_ignore_ascii_case("user_manager"));
    let system_manager_role = effective_roles
        .iter()
        .any(|role| role.eq_ignore_ascii_case("system_manager"));

    if rbac_enabled
        && (!effective_roles.is_empty())
        && (admin_role != user.admin
            || user_manager_role != user.user_manager
            || system_manager_role != user.system_manager)
    {
        let update = UpdatePermissions {
            admin: Some(admin_role),
            user_manager: Some(user_manager_role),
            system_manager: Some(system_manager_role),
            default_repository_actions: None,
            repository_permissions: Default::default(),
        };
        if let Err(err) = update.update_permissions(user.id, &site.database).await {
            warn!(%err, user_id = user.id, "Failed to synchronize OAuth2 user flags");
        } else {
            user.admin = admin_role;
            user.user_manager = user_manager_role;
            user.system_manager = system_manager_role;
        }
    }

    let user_agent = user_agent
        .map(|ua| ua.to_string())
        .unwrap_or_else(|| "Pkgly OAuth2".to_string());
    let ip = addr.ip().to_string();
    let session =
        match site
            .session_manager
            .create_session(user.id, user_agent, ip, Duration::days(1))
        {
            Ok(session) => session,
            Err(err) => {
                error!(%err, "Failed to create session for OAuth2 login");
                return Ok(err.into_response());
            }
        };

    let is_https = site.instance.lock().is_https;
    let cookie = session_cookie(session.session_id.clone(), is_https);

    let redirect_header =
        sanitize_redirect(exchange.redirect.as_deref().or(query.redirect.as_deref()));

    let redirect_str = redirect_header.to_str().unwrap_or("/").to_string();
    info!(
        user_id = user.id,
        email = ?user.email,
        provider = %exchange.provider,
        redirect = %redirect_str,
        "OAuth2 login succeeded"
    );

    let response = ResponseBuilder::default()
        .status(StatusCode::SEE_OTHER)
        .header(SET_COOKIE, cookie.encoded().to_string())
        .header(LOCATION, redirect_header)
        .empty();

    access_log.set_user(user.username.as_ref().to_string());
    access_log.set_user_id(user.id);

    Ok(response)
}

fn resolve_base_url(site: &Pkgly) -> Option<String> {
    if let Some(settings) = site.oauth2_settings_raw() {
        if let Some(base) = settings.redirect_base_url.clone() {
            if !base.is_empty() {
                return Some(base);
            }
        }
    }

    let instance = site.inner.instance.lock();
    if !instance.app_url.is_empty() {
        return Some(instance.app_url.clone());
    }
    None
}

async fn resolve_oauth_user(
    site: &Pkgly,
    settings: &OAuth2Settings,
    principal: &SsoPrincipal,
) -> Result<UserSafeData, Response> {
    if let Some(email) = principal.email.as_ref() {
        if !principal.email_verified {
            warn!(
                email = %email,
                username = %principal.username,
                "OAuth2 email lookup skipped: email not verified by identity provider"
            );
        } else {
            match UserSafeData::get_by_email(email, &site.database).await {
                Ok(Some(user)) => return Ok(user),
                Ok(None) => {}
                Err(err) => {
                    error!(%err, "Failed to lookup user by email during OAuth2 login");
                    return Err(internal_login_error());
                }
            }
        }
    }

    match UserSafeData::get_by_username_or_email(&principal.username, &site.database).await {
        Ok(Some(user)) => return Ok(user),
        Ok(None) => {}
        Err(err) => {
            error!(%err, "Failed to lookup user by username during OAuth2 login");
            return Err(internal_login_error());
        }
    }

    if !settings.auto_create_users {
        return Err(oauth_denied_redirect("no_account"));
    }

    if principal.email.is_some() && !principal.email_verified {
        warn!(
            email = %principal.email.as_deref().unwrap_or("?"),
            username = %principal.username,
            "OAuth2 auto-create rejected: email not verified"
        );
        return Err(oauth_denied_redirect("unverified_email"));
    }

    create_user(site, principal).await
}

async fn persist_oauth_state(
    site: &Pkgly,
    state: &str,
    snapshot: &OAuthStateExport,
) -> Result<(), InternalError> {
    prune_persisted_oauth_states(site).await?;
    sqlx::query(
        r#"
        INSERT INTO oauth2_states (state, provider, pkce_verifier, redirect, nonce, callback_uri, config_fingerprint)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (state) DO UPDATE
        SET provider = EXCLUDED.provider,
            pkce_verifier = EXCLUDED.pkce_verifier,
            redirect = EXCLUDED.redirect,
            nonce = EXCLUDED.nonce, callback_uri = EXCLUDED.callback_uri, config_fingerprint = EXCLUDED.config_fingerprint,
            created_at = NOW()
        "#,
    )
    .bind(state)
    .bind(snapshot.provider.to_string())
    .bind(snapshot.pkce_verifier.as_str())
    .bind(snapshot.redirect.as_deref())
    .bind(snapshot.nonce.as_deref())
    .bind(snapshot.callback_uri.as_deref())
    .bind(snapshot.config_fingerprint.as_deref())
    .execute(&site.database)
    .await?;
    Ok(())
}

async fn load_persisted_oauth_state(
    site: &Pkgly,
    state: &str,
) -> Result<Option<PersistedOAuthState>, InternalError> {
    let row = sqlx::query(
        r#"
        DELETE FROM oauth2_states
        WHERE state = $1
        RETURNING provider, pkce_verifier, redirect, created_at, nonce, callback_uri, config_fingerprint
        "#,
    )
    .bind(state)
    .fetch_optional(&site.database)
    .await?;

    Ok(row.map(|record| PersistedOAuthState {
        provider: record.get::<String, _>("provider"),
        pkce_verifier: record.get::<String, _>("pkce_verifier"),
        redirect: record.get::<Option<String>, _>("redirect"),
        created_at: record.get::<DateTime<Utc>, _>("created_at"),
        nonce: record.get("nonce"),
        callback_uri: record.get("callback_uri"),
        config_fingerprint: record.get("config_fingerprint"),
    }))
}

async fn prune_persisted_oauth_states(site: &Pkgly) -> Result<(), InternalError> {
    sqlx::query(
        r#"
        DELETE FROM oauth2_states
        WHERE created_at < NOW() - INTERVAL '15 minutes'
        "#,
    )
    .execute(&site.database)
    .await?;
    Ok(())
}

fn build_principal(claims: &IdTokenClaims) -> SsoPrincipal {
    let source_username = claims
        .preferred_username
        .clone()
        .or_else(|| claims.email.clone())
        .unwrap_or_else(|| claims.sub.clone());
    let username = normalize_username(&source_username);

    let display_name = claims
        .name
        .clone()
        .or_else(|| claims.given_name.clone())
        .or_else(|| claims.email.clone())
        .unwrap_or_else(|| username.clone());

    SsoPrincipal {
        username,
        email: claims.email.clone(),
        email_verified: claims.email_verified.unwrap_or(false),
        display_name,
        roles: Vec::new(),
    }
}

fn oauth_service_error_response(err: OAuth2ServiceError) -> Response {
    match err {
        OAuth2ServiceError::Disabled
        | OAuth2ServiceError::MissingProviders
        | OAuth2ServiceError::ProviderNotConfigured(_) => {
            ResponseBuilder::not_found().body("OAuth2 provider not available")
        }
        OAuth2ServiceError::InvalidState => {
            ResponseBuilder::bad_request().body("OAuth2 login state is invalid or has expired")
        }
        OAuth2ServiceError::InvalidRedirectUrl(details)
        | OAuth2ServiceError::ClientConstruction(details) => {
            let api_error: APIErrorResponse<String, ()> = APIErrorResponse {
                message: "OAuth2 configuration error".into(),
                details: Some(details),
                error: None,
            };
            ResponseBuilder::internal_server_error().json(&api_error)
        }
        OAuth2ServiceError::TokenRequestFailed(details) => {
            let api_error: APIErrorResponse<String, ()> = APIErrorResponse {
                message: "OAuth2 provider rejected the request".into(),
                details: Some(details),
                error: None,
            };
            ResponseBuilder::internal_server_error().json(&api_error)
        }
    }
}

fn oauth_denied_redirect(reason: &str) -> Response {
    let location = format!("/oauth/denied?reason={reason}");
    ResponseBuilder::default()
        .status(StatusCode::SEE_OTHER)
        .header(LOCATION, location)
        .empty()
}

fn internal_login_error() -> Response {
    let api_error: APIErrorResponse<(), ()> = APIErrorResponse {
        message: "Unexpected error processing OAuth2 login".into(),
        details: None,
        error: None,
    };
    ResponseBuilder::internal_server_error().json(&api_error)
}

fn extract_roles(provider: OAuth2ProviderKind, claims: &IdTokenClaims) -> Vec<String> {
    let mut collected = Vec::new();
    if let Some(roles) = claims.roles.as_ref() {
        collected.extend(roles.iter().cloned());
    }
    if let Some(groups) = claims.groups.as_ref() {
        collected.extend(groups.iter().cloned());
    }

    if collected.is_empty()
        && provider == OAuth2ProviderKind::Google
        && claims.email_verified == Some(true)
    {
        collected.extend(claims.email.iter().map(|email| format!("group:{email}")));
    }

    collected.retain(|role| !role.trim().is_empty());
    collected.sort();
    collected.dedup();
    collected
}

fn map_roles_from_claims(
    provider: OAuth2ProviderKind,
    claims: &[String],
    mappings: &[OAuth2GroupRoleMapping],
) -> Vec<String> {
    let mut assigned = Vec::new();
    for mapping in mappings.iter().filter(|m| m.provider == provider) {
        let group = mapping.group.trim();
        if group.is_empty() || mapping.roles.is_empty() {
            continue;
        }
        if claims.iter().any(|claim| claim.eq_ignore_ascii_case(group)) {
            assigned.extend(mapping.roles.clone());
        }
    }
    assigned
}

#[cfg(test)]
mod tests;

async fn verify_builtin_token(
    provider: &OAuth2ProviderKind,
    settings: &OAuth2Settings,
    id_token: &str,
    jwks_manager: &JwksManager<ReqwestJwksFetcher>,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let provider_config = match provider {
        OAuth2ProviderKind::Google => OidcProviderConfig {
            name: "google-oauth2".to_string(),
            issuer: "https://accounts.google.com".to_string(),
            audience: settings
                .google
                .as_ref()
                .map(|g| g.client_id.clone())
                .unwrap_or_default(),
            jwks_url: Some("https://www.googleapis.com/oauth2/v3/certs".to_string()),
            token_source: TokenSource::Header {
                name: "Authorization".to_string(),
                prefix: Some("Bearer ".to_string()),
            },
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: Vec::new(),
        },
        OAuth2ProviderKind::Microsoft => OidcProviderConfig {
            name: "microsoft-oauth2".to_string(),
            issuer: "https://login.microsoftonline.com/common/v2.0".to_string(),
            audience: settings
                .microsoft
                .as_ref()
                .map(|m| m.client_id.clone())
                .unwrap_or_default(),
            jwks_url: Some(
                "https://login.microsoftonline.com/common/discovery/v2.0/keys".to_string(),
            ),
            token_source: TokenSource::Header {
                name: "Authorization".to_string(),
                prefix: Some("Bearer ".to_string()),
            },
            subject_claim: None,
            email_claim: None,
            display_name_claim: None,
            role_claims: Vec::new(),
        },
        OAuth2ProviderKind::Custom(_) => return Err("Unexpected generic provider".into()),
    };

    jwks_manager
        .verify(id_token, &provider_config)
        .await
        .map_err(|_| "Invalid builtin identity token".into())
}
