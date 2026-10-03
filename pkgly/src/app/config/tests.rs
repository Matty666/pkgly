// ABOUTME: Tests validation and normalization of application configuration values.
// ABOUTME: Covers trusted site URL requirements used by security-sensitive links.
use super::{ConfigError, normalize_app_url};

#[test]
fn normalizes_site_url_path() {
    assert_eq!(
        normalize_app_url("https://panel.example/pkgly").unwrap(),
        "https://panel.example/pkgly/"
    );
}

#[test]
fn rejects_untrusted_site_url_shapes() {
    for value in [
        "javascript:alert(1)",
        "https://user:pass@panel.example/",
        "https://panel.example/?redirect=evil",
        "https://panel.example/#fragment",
    ] {
        assert!(matches!(
            normalize_app_url(value),
            Err(ConfigError::InvalidAppUrl(_))
        ));
    }
}

#[test]
fn generic_oidc_configuration_preserves_builtins_and_custom_ids() {
    let settings: super::OAuth2Settings = serde_json::from_value(serde_json::json!({
        "google": {"client_id": "google-client", "client_secret": "google-secret"},
        "providers": [{"id": "company-sso", "display_name": "Company", "issuer": "https://id.example/team",
            "client_id": "pkgly", "client_secret": "secret"}]
    })).unwrap();
    settings.validate_providers().unwrap();
    assert!(settings.google.is_some());
    assert_eq!(settings.providers[0].id, "company-sso");
    let kind: super::OAuth2ProviderKind = serde_json::from_str("\"company-sso\"").unwrap();
    assert_eq!(kind.to_string(), "company-sso");
    assert_eq!(serde_json::to_string(&kind).unwrap(), "\"company-sso\"");
    assert!(super::OAuth2Settings::default().providers.is_empty());
}

#[test]
fn generic_oidc_configuration_rejects_invalid_contracts() {
    let valid = serde_json::json!({"id": "company-sso", "display_name": "Company", "issuer": "https://id.example",
        "client_id": "pkgly", "client_secret": "secret"});
    for (field, value) in [
        ("id", "Google"),
        ("id", "entra_id"),
        ("id", "../sso"),
        ("id", ""),
        ("issuer", "http://id.example"),
        ("issuer", "https://user:pass@id.example"),
        ("id_token_signing_alg", "HS256"),
        ("token_endpoint_auth_method", "none"),
    ] {
        let mut provider = valid.clone();
        provider[field] = value.into();
        let parsed = serde_json::from_value::<super::OAuth2Settings>(
            serde_json::json!({"providers": [provider]}),
        );
        assert!(
            parsed.is_err() || parsed.unwrap().validate_providers().is_err(),
            "{field}: {value}"
        );
    }
    for providers in [
        serde_json::json!([valid.clone(), valid.clone()]),
        serde_json::json!([{ "endpoints": {"authorization_url":"https://id.example/auth"},
            "id":"other", "issuer":"https://id.example", "client_id":"pkgly", "client_secret":"secret"}]),
        serde_json::json!([{ "id":"other", "issuer":"https://id.example", "client_id":"pkgly", "client_secret":"secret", "scopes":["email"]}]),
    ] {
        let parsed = serde_json::from_value::<super::OAuth2Settings>(
            serde_json::json!({"providers":providers}),
        );
        assert!(parsed.is_err() || parsed.unwrap().validate_providers().is_err());
    }
}
