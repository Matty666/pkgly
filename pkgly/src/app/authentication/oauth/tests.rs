#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

fn google_settings() -> OAuth2Settings {
    let mut settings = OAuth2Settings::default();
    settings.enabled = true;
    settings.redirect_base_url = Some("https://app.example.com".to_string());
    settings.google = Some(OAuth2GoogleConfig {
        client_id: "client-id".to_string(),
        client_secret: "client-secret".to_string(),
        scopes: vec!["openid".to_string(), "profile".to_string()],
        redirect_path: None,
    });
    settings
}

#[test]
fn normalize_scopes_returns_defaults_when_empty() {
    let scopes = Vec::<String>::new();
    let normalized = normalize_scopes(&scopes);
    assert_eq!(normalized, vec!["openid", "profile", "email"]);
}

#[test]
fn normalize_scopes_deduplicates_and_preserves_order() {
    let scopes = vec![
        "email".to_string(),
        "profile".to_string(),
        "email".to_string(),
        String::new(),
    ];
    let normalized = normalize_scopes(&scopes);
    assert_eq!(normalized, vec!["email", "profile"]);
}

#[tokio::test]
async fn oauth_service_generates_authorization_url() {
    let settings = google_settings();
    let service = OAuth2Service::new(settings.clone())
        .expect("service construction should succeed")
        .expect("service should be available");

    let redirect = service
        .begin_authorization(
            OAuth2ProviderKind::Google,
            settings.redirect_base_url.as_deref(),
            Some("/welcome".to_string()),
        )
        .await
        .expect("authorization URL should be generated");

    assert!(
        redirect
            .authorization_url
            .as_str()
            .starts_with("https://accounts.google.com")
    );
    assert!(!redirect.state.is_empty());
}

#[test]
fn oauth_service_rejects_missing_providers() {
    let mut settings = OAuth2Settings::default();
    settings.enabled = true;
    let result = OAuth2Service::new(settings);
    assert!(result.is_err());
}

#[tokio::test]
async fn generic_oidc_authorization_uses_configuration_and_persists_nonce() {
    for id in ["company-sso", "partner-login"] {
        let settings: OAuth2Settings = serde_json::from_value(serde_json::json!({
            "enabled":true, "redirect_base_url":"https://pkgly.example",
            "providers":[{"id":id,"display_name":"Custom login","issuer":"https://id.example/team",
                "client_id":"client", "client_secret":"secret",
                "endpoints":{"authorization_url":"https://id.example/unusual/authorize",
                    "token_url":"https://id.example/unusual/token","jwks_url":"https://id.example/keys"}}]
        })).unwrap();
        let service = OAuth2Service::new(settings).unwrap().unwrap();
        let provider = id.parse().unwrap();
        let redirect = service
            .begin_authorization(provider, None, Some("/browse".into()))
            .await
            .unwrap();
        assert_eq!(redirect.authorization_url.path(), "/unusual/authorize");
        let params: std::collections::HashMap<_, _> =
            redirect.authorization_url.query_pairs().collect();
        assert_eq!(params["client_id"], "client");
        assert_eq!(params["code_challenge_method"], "S256");
        let snapshot = service.export_state(&redirect.state).unwrap();
        assert_eq!(snapshot.provider.to_string(), id);
        assert_eq!(snapshot.nonce.as_deref(), Some(params["nonce"].as_ref()));
        assert_eq!(
            snapshot.callback_uri.as_deref(),
            Some("https://pkgly.example/api/user/oauth2/callback")
        );
        assert!(snapshot.config_fingerprint.is_some());
        let info = service.provider_descriptors();
        assert_eq!(info[0].display_name, "Custom login");
    }
}

#[tokio::test]
async fn generic_oidc_disabled_provider_is_not_advertised() {
    let mut settings = google_settings();
    settings.providers =
        serde_json::from_value(serde_json::json!([{"id":"unused", "enabled":false}])).unwrap();
    let service = OAuth2Service::new(settings).unwrap().unwrap();
    assert_eq!(service.provider_descriptors().len(), 1);
    assert!(
        service
            .begin_authorization("unused".parse().unwrap(), None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn generic_oidc_rejects_missing_nonce_and_removed_provider_before_exchange() {
    let settings: OAuth2Settings=serde_json::from_value(serde_json::json!({"enabled":true,"redirect_base_url":"https://pkgly.example",
        "providers":[{"id":"company","display_name":"Company","issuer":"https://id.example","client_id":"pkgly","client_secret":"secret",
            "endpoints":{"authorization_url":"https://id.example/auth","token_url":"https://id.example/token","jwks_url":"https://id.example/keys"}}]})).unwrap();
    let service = OAuth2Service::new(settings).unwrap().unwrap();
    let auth = service
        .begin_authorization("company".parse().unwrap(), None, None)
        .await
        .unwrap();
    let snapshot = service.export_state(&auth.state).unwrap();
    for missing in [
        "nonce",
        "callback_uri",
        "config_fingerprint",
        "changed_callback",
    ] {
        let mut invalid = snapshot.clone();
        match missing {
            "nonce" => invalid.nonce = None,
            "callback_uri" => invalid.callback_uri = None,
            "config_fingerprint" => invalid.config_fingerprint = None,
            _ => invalid.callback_uri = Some("https://other.example/callback".into()),
        }
        assert!(
            matches!(
                service
                    .exchange_code_with_export(None, AuthorizationCode::new("code".into()), invalid)
                    .await,
                Err(OAuth2ServiceError::InvalidState)
            ),
            "{missing}"
        );
    }
    let removed = OAuth2Service::new(google_settings()).unwrap().unwrap();
    assert!(matches!(
        removed
            .exchange_code_with_export(None, AuthorizationCode::new("code".into()), snapshot)
            .await,
        Err(OAuth2ServiceError::ProviderNotConfigured(_))
    ));
}

#[tokio::test]
async fn generic_oidc_can_disable_the_last_custom_provider() {
    let settings: OAuth2Settings = serde_json::from_value(serde_json::json!({
        "enabled": true, "providers": [{"id":"company", "enabled":false}]
    }))
    .unwrap();
    let service = OAuth2Service::new(settings).unwrap().unwrap();
    assert!(service.provider_descriptors().is_empty());
    assert!(
        service
            .begin_authorization("company".parse().unwrap(), None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn generic_oidc_discovers_on_each_login_attempt() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = Arc::new(AtomicUsize::new(0));
    let discovery_attempts = attempts.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let endpoint_issuer = issuer.clone();
    let app = axum::Router::new().route(
        "/.well-known/openid-configuration",
        axum::routing::get(move || {
            let issuer = endpoint_issuer.clone();
            let attempt = discovery_attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                let status = if attempt == 0 {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                } else {
                    axum::http::StatusCode::OK
                };
                (
                    status,
                    axum::Json(serde_json::json!({
                        "issuer": issuer,
                        "authorization_endpoint": format!("{issuer}/authorize/{attempt}"),
                        "token_endpoint": format!("{issuer}/token"),
                        "jwks_uri": format!("{issuer}/keys"),
                        "response_types_supported": ["code"],
                        "id_token_signing_alg_values_supported": ["RS256"],
                    })),
                )
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut settings = google_settings();
    settings.providers = serde_json::from_value(serde_json::json!([{
        "id": "company", "display_name": "Company", "issuer": issuer,
        "client_id": "client", "client_secret": "secret"
    }]))
    .unwrap();
    let service = OAuth2Service::new(settings).unwrap().unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    assert_eq!(service.provider_descriptors().len(), 2);
    let provider: OAuth2ProviderKind = "company".parse().unwrap();
    assert!(
        service
            .begin_authorization(provider.clone(), None, None)
            .await
            .is_err()
    );
    assert_eq!(service.provider_descriptors().len(), 2);
    for attempt in 1..=2 {
        let redirect = service
            .begin_authorization(provider.clone(), None, None)
            .await
            .unwrap();
        assert_eq!(
            redirect.authorization_url.path(),
            format!("/authorize/{attempt}")
        );
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    server.abort();
}
