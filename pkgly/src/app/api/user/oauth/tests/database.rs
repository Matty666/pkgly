use super::*;
use crate::app::config::{Mode, SecuritySettings, SiteSetting};
use nr_core::{
    database::{DatabaseConfig, entities::user::NewUserRequest},
    user::{Email, Username},
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use testcontainers::{clients::Cli, images::generic::GenericImage};

async fn test_site(port: u16, root: &std::path::Path) -> Pkgly {
    Pkgly::new(
        Mode::Debug,
        SiteSetting::default(),
        SecuritySettings::default(),
        crate::app::authentication::session::SessionManagerConfig {
            database_location: root.join("sessions.redb"),
            ..Default::default()
        },
        crate::repository::StagingConfig {
            staging_dir: root.join("staging"),
            ..Default::default()
        },
        None,
        DatabaseConfig {
            user: "postgres".into(),
            password: "password".into(),
            database: "postgres".into(),
            host: "127.0.0.1".into(),
            port: Some(port),
        },
        Some(root.join("storages")),
    )
    .await
    .unwrap()
}

async fn user(site: &Pkgly, username: &str, email: &str) -> i32 {
    NewUserRequest {
        name: username.into(),
        username: Username::new(username.into()).unwrap(),
        email: Some(Email::new(email.into()).unwrap()),
        password: None,
        permissions: None,
    }
    .insert(&site.database)
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn generic_oidc_database_login_state_and_account_policy() {
    let _lock = crate::test_support::DB_TEST_LOCK.lock().await;
    let docker = Cli::default();
    let container = docker.run(
        GenericImage::new("postgres", "18-alpine")
            .with_env_var("POSTGRES_PASSWORD", "password")
            .with_env_var("POSTGRES_USER", "postgres")
            .with_env_var("POSTGRES_DB", "postgres"),
    );
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:password@127.0.0.1:{port}/postgres");
    for attempt in 0..60 {
        if PgPoolOptions::new().connect(&url).await.is_ok() {
            break;
        }
        assert!(attempt < 59, "postgres failed to start");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let root = tempfile::tempdir().unwrap();
    let site = test_site(port, root.path()).await;
    let first = user(&site, "first", "first@example.com").await;
    let second = user(&site, "second", "second@example.com").await;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{
        RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts,
    };
    let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
    let encoding =
        jsonwebtoken::EncodingKey::from_rsa_der(private.to_pkcs1_der().unwrap().as_bytes());
    let keys = json!({"keys":[{"kid":"test-key","kty":"RSA","n":URL_SAFE_NO_PAD.encode(private.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(private.e().to_bytes_be())}]});
    let token = Arc::new(parking_lot::Mutex::new(String::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let metadata = json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/arbitrary/auth"),
        "token_endpoint":format!("{issuer}/arbitrary/token"),"jwks_uri":format!("{issuer}/arbitrary/keys"),
        "response_types_supported":["code"],"id_token_signing_alg_values_supported":["RS256"],"token_endpoint_auth_methods_supported":["client_secret_basic","client_secret_post"]});
    let token_server = token.clone();
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let requests_server = requests.clone();
    let app=axum::Router::new()
        .route("/.well-known/openid-configuration",axum::routing::get(move || { let metadata=metadata.clone(); async move { axum::Json(metadata) } }))
        .route("/arbitrary/keys",axum::routing::get(move || { let keys=keys.clone(); async move { axum::Json(keys) } }))
        .route("/arbitrary/token",axum::routing::post(move |headers: axum::http::HeaderMap, axum::Form(form): axum::Form<std::collections::HashMap<String, String>>| {
            let token=token_server.lock().clone();
            requests_server.lock().push((headers, form));
            async move {
            axum::Json(json!({"access_token":"test-access","token_type":"Bearer","id_token":token}))
        } }));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let settings: OAuth2Settings=serde_json::from_value(json!({"enabled":true,"redirect_base_url":"https://pkgly.example","casbin":null,
        "providers":[{"id":"company","display_name":"Company sign-in","issuer":issuer,"client_id":"pkgly","client_secret":"secret"}]})).unwrap();
    site.update_oauth2_settings(Some(settings.clone()))
        .await
        .unwrap();
    let public = list_providers(State(site.clone())).await.unwrap();
    assert_eq!(public.status(), StatusCode::OK);

    // Verified email wins over a different username. Missing/false verification uses the existing username fallback.
    for (verified, expected, inactive, method) in [
        (Some(true), first, false, "client_secret_basic"),
        (Some(false), second, false, "client_secret_post"),
        (None, second, false, "client_secret_basic"),
        (Some(true), first, true, "client_secret_post"),
    ] {
        let mut case_settings = settings.clone();
        case_settings.providers[0].token_endpoint_auth_method = method.into();
        site.update_oauth2_settings(Some(case_settings.clone()))
            .await
            .unwrap();
        sqlx::query("UPDATE users SET active = $1 WHERE id = $2")
            .bind(!inactive)
            .bind(first)
            .execute(&site.database)
            .await
            .unwrap();
        let service = site.oauth2_service().unwrap();
        let auth = service
            .begin_authorization("company".parse().unwrap(), None, Some("/browse".into()))
            .unwrap();
        let snapshot = service.export_state(&auth.state).unwrap();
        persist_oauth_state(&site, &auth.state, &snapshot)
            .await
            .unwrap();
        let now = Utc::now().timestamp();
        let mut claims = json!({"iss":issuer,"aud":"pkgly","sub":"opaque","iat":now,"exp":now+300,
            "nonce":snapshot.nonce,"preferred_username":"second","email":"first@example.com","name":"Standard user"});
        if let Some(verified) = verified {
            claims["email_verified"] = json!(verified);
        }
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some("test-key".into());
        *token.lock() = jsonwebtoken::encode(&header, &claims, &encoding).unwrap();
        // Replace the runtime to exercise callback recovery from database state.
        site.update_oauth2_settings(Some(case_settings))
            .await
            .unwrap();
        let access = AccessLogContext::default();
        let response = callback(
            State(site.clone()),
            Extension(access.clone()),
            ConnectInfo("127.0.0.1:1234".parse().unwrap()),
            None,
            Query(OAuthCallbackQuery {
                code: Some("code".into()),
                state: Some(auth.state.clone()),
                error: None,
                error_description: None,
                redirect: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        if inactive {
            assert!(
                response.headers()[LOCATION]
                    .to_str()
                    .unwrap()
                    .contains("inactive")
            );
            assert!(!response.headers().contains_key(SET_COOKIE));
        } else {
            assert_eq!(response.headers()[LOCATION], "/browse");
            assert!(response.headers().contains_key(SET_COOKIE));
            assert_eq!(access.snapshot().user_id, Some(expected));
        }
        let (headers, form) = requests.lock().pop().unwrap();
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(
            form["redirect_uri"],
            "https://pkgly.example/api/user/oauth2/callback"
        );
        assert_eq!(form["code_verifier"], snapshot.pkce_verifier);
        if method == "client_secret_basic" {
            assert!(
                headers[axum::http::header::AUTHORIZATION]
                    .to_str()
                    .unwrap()
                    .starts_with("Basic ")
            );
            assert!(!form.contains_key("client_secret"));
        } else {
            assert!(!headers.contains_key(axum::http::header::AUTHORIZATION));
            assert_eq!(form["client_id"], "pkgly");
            assert_eq!(form["client_secret"], "secret");
        }
        assert!(
            load_persisted_oauth_state(&site, &auth.state)
                .await
                .unwrap()
                .is_none()
        );
    }
    site.update_oauth2_settings(Some(settings.clone()))
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&site.database)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let unknown = SsoPrincipal {
        username: "missing".into(),
        email: Some("missing@example.com".into()),
        email_verified: true,
        display_name: "Missing".into(),
        roles: vec![],
    };
    let denied = resolve_oauth_user(&site, &settings, &unknown)
        .await
        .unwrap_err();
    assert!(
        denied.headers()[LOCATION]
            .to_str()
            .unwrap()
            .contains("no_account")
    );
    let mut creation = settings.clone();
    creation.auto_create_users = true;
    let mut unverified = unknown;
    unverified.email_verified = false;
    let denied = resolve_oauth_user(&site, &creation, &unverified)
        .await
        .unwrap_err();
    assert!(
        denied.headers()[LOCATION]
            .to_str()
            .unwrap()
            .contains("unverified_email")
    );

    let service = site.oauth2_service().unwrap();
    let auth = service
        .begin_authorization("company".parse().unwrap(), None, None)
        .unwrap();
    let snapshot = service.export_state(&auth.state).unwrap();
    persist_oauth_state(&site, &auth.state, &snapshot)
        .await
        .unwrap();
    let (left, right) = tokio::join!(
        load_persisted_oauth_state(&site, &auth.state),
        load_persisted_oauth_state(&site, &auth.state)
    );
    assert_eq!(
        usize::from(left.unwrap().is_some()) + usize::from(right.unwrap().is_some()),
        1
    );

    // Settings validation and persistence failure must leave the prior runtime usable.
    let mut changed = settings.clone();
    changed.providers[0].client_secret = "replacement".into();
    let replacement = crate::app::authentication::oauth::OAuth2Service::initialize(changed.clone())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        replacement
            .exchange_code_with_export(None, AuthorizationCode::new("code".into()), snapshot)
            .await,
        Err(OAuth2ServiceError::InvalidState)
    ));
    sqlx::query("ALTER TABLE application_settings RENAME TO hidden_settings")
        .execute(&site.database)
        .await
        .unwrap();
    assert!(site.update_oauth2_settings(Some(changed)).await.is_err());
    assert_eq!(
        site.oauth2_settings_raw().unwrap().providers[0].client_secret,
        "secret"
    );
    assert_eq!(
        site.oauth2_service().unwrap().provider_descriptors()[0].provider,
        "company"
    );
    sqlx::query("ALTER TABLE hidden_settings RENAME TO application_settings")
        .execute(&site.database)
        .await
        .unwrap();
    let mut invalid = settings.clone();
    invalid.providers[0].issuer = format!("{issuer}/missing");
    assert!(site.update_oauth2_settings(Some(invalid)).await.is_err());
    assert_eq!(
        site.oauth2_settings_raw().unwrap().providers[0].issuer,
        issuer
    );
    server.abort();
}
