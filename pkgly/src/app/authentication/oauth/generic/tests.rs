#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::*;
use serde_json::json;

#[test]
fn generic_oidc_metadata_requires_exact_issuer_and_supported_options() {
    let config = OAuth2GenericConfig {
        id: "company".into(),
        display_name: "Company".into(),
        issuer: "https://issuer.example/team".into(),
        client_id: "client".into(),
        client_secret: "secret".into(),
        ..Default::default()
    };
    let valid = json!({"issuer": config.issuer, "authorization_endpoint":"https://issuer.example/auth",
        "token_endpoint":"https://issuer.example/token", "jwks_uri":"https://issuer.example/keys",
        "response_types_supported":["code"], "id_token_signing_alg_values_supported":["RS256"]});
    assert!(resolve_metadata(&config, valid.clone()).is_ok());
    for (field, value) in [
        ("issuer", json!("https://issuer.example/team/")),
        ("token_endpoint", json!("http://issuer.example/token")),
        ("jwks_uri", json!(null)),
        ("authorization_endpoint", json!(null)),
        ("id_token_signing_alg_values_supported", json!(["HS256"])),
        ("response_types_supported", json!(["token"])),
        ("grant_types_supported", json!(["client_credentials"])),
        ("code_challenge_methods_supported", json!(["plain"])),
        (
            "token_endpoint_auth_methods_supported",
            json!(["private_key_jwt"]),
        ),
    ] {
        let mut metadata = valid.clone();
        metadata[field] = value;
        assert!(resolve_metadata(&config, metadata).is_err(), "{field}");
    }
}

#[test]
fn generic_oidc_signed_tokens_require_signature_nonce_and_identity_claims() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{
        RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts,
    };
    let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
    let encoding =
        jsonwebtoken::EncodingKey::from_rsa_der(private.to_pkcs1_der().unwrap().as_bytes());
    let decoding = jsonwebtoken::DecodingKey::from_rsa_components(
        &URL_SAFE_NO_PAD.encode(private.n().to_bytes_be()),
        &URL_SAFE_NO_PAD.encode(private.e().to_bytes_be()),
    )
    .unwrap();
    let config = OAuth2GenericConfig {
        issuer: "https://issuer.example".into(),
        client_id: "pkgly".into(),
        ..Default::default()
    };
    let now = chrono::Utc::now().timestamp();
    let valid = json!({"iss":config.issuer,"aud":"pkgly","sub":"user","iat":now,"exp":now+300,"nonce":"saved"});
    let sign = |claims: &serde_json::Value| {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            claims,
            &encoding,
        )
        .unwrap()
    };
    assert!(verify_claims(&sign(&valid), &decoding, &config, "saved").is_ok());
    for (field, value) in [
        ("iss", json!("https://other.example")),
        ("aud", json!("other")),
        ("exp", json!(now - 100)),
        ("nonce", json!("wrong")),
        ("iat", json!(now + 300)),
        ("sub", json!("")),
        ("azp", json!("other")),
        ("aud", json!(["pkgly", "other"])),
    ] {
        let mut claims = valid.clone();
        claims[field] = value;
        assert!(
            verify_claims(&sign(&claims), &decoding, &config, "saved").is_err(),
            "{field}"
        );
    }
    for field in ["iss", "aud", "exp", "sub", "iat", "nonce"] {
        let mut claims = valid.clone();
        claims.as_object_mut().unwrap().remove(field);
        assert!(
            verify_claims(&sign(&claims), &decoding, &config, "saved").is_err(),
            "missing {field}"
        );
    }
    let mut claims = valid.clone();
    claims["aud"] = json!(["pkgly"]);
    assert!(verify_claims(&sign(&claims), &decoding, &config, "saved").is_ok());
    let wrong = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &valid,
        &jsonwebtoken::EncodingKey::from_secret(b"secret"),
    )
    .unwrap();
    assert!(verify_claims(&wrong, &decoding, &config, "saved").is_err());
    let bad_key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
    let bad = jsonwebtoken::EncodingKey::from_rsa_der(bad_key.to_pkcs1_der().unwrap().as_bytes());
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &valid,
        &bad,
    )
    .unwrap();
    assert!(verify_claims(&token, &decoding, &config, "saved").is_err());
}

#[tokio::test]
async fn generic_oidc_discovery_rejects_malformed_timeout_and_blocked_requests() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route(
            "/bad/.well-known/openid-configuration",
            axum::routing::get(|| async { "invalid JSON" }),
        )
        .route(
            "/slow/.well-known/openid-configuration",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "{}"
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = crate::utils::upstream::client_builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let mut config = OAuth2GenericConfig {
        issuer: format!("{issuer}/bad"),
        ..Default::default()
    };
    assert_eq!(
        discover(&config, &client).await.unwrap_err(),
        "Invalid OIDC discovery metadata"
    );
    config.issuer = format!("{issuer}/slow");
    assert_eq!(
        discover(&config, &client).await.unwrap_err(),
        "OIDC discovery request failed"
    );
    config.issuer = "https://10.1.2.3".into();
    assert!(crate::utils::egress::validate_url(&Url::parse(&config.issuer).unwrap()).is_err());
    assert_eq!(
        discover(&config, &client).await.unwrap_err(),
        "OIDC discovery request failed"
    );
    server.abort();
}
