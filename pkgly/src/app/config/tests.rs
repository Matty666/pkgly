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
        "generic_providers": [{"id": "company-sso", "display_name": "Company", "issuer": "https://id.example/team",
            "client_id": "pkgly", "client_secret": "secret", "endpoints":{"authorization_url":"https://id.example/auth","token_url":"https://id.example/token","jwks_url":"https://keys.example/jwks"}}]
    })).unwrap();
    settings.validate_providers().unwrap();
    assert!(settings.google.is_some());
    assert_eq!(settings.generic_providers[0].id, "company-sso");
    let kind: super::OAuth2ProviderKind = serde_json::from_str("\"company-sso\"").unwrap();
    assert_eq!(kind.to_string(), "company-sso");
    assert_eq!(serde_json::to_string(&kind).unwrap(), "\"company-sso\"");
    assert!(
        super::OAuth2Settings::default()
            .generic_providers
            .is_empty()
    );
}

#[test]
fn generic_oidc_configuration_rejects_invalid_contracts() {
    let valid = serde_json::json!({"id": "company-sso", "display_name": "Company", "issuer": "https://id.example",
        "client_id": "pkgly", "client_secret": "secret", "endpoints":{"authorization_url":"https://id.example/auth","token_url":"https://id.example/token","jwks_url":"https://keys.example/jwks"}});
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
            serde_json::json!({"generic_providers": [provider]}),
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
            serde_json::json!({"generic_providers":providers}),
        );
        assert!(parsed.is_err() || parsed.unwrap().validate_providers().is_err());
    }
}

#[test]
fn generic_oidc_role_mappings_require_configured_custom_provider_ids() {
    use super::OAuth2Settings;
    let mut settings: OAuth2Settings = serde_json::from_value(serde_json::json!({
        "generic_providers": [{"id":"company", "enabled":false}],
        "group_role_mappings": [
            {"provider":"company", "group":"staff", "roles":["read"]},
            {"provider":"google", "group":"existing", "roles":["read"]}
        ]
    }))
    .unwrap();
    assert!(settings.validate_providers().is_ok());
    settings.generic_providers.clear();
    assert!(settings.validate_providers().is_err());
    settings.group_role_mappings.remove(0);
    assert!(settings.validate_providers().is_ok());
}

#[test]
fn enabled_generic_oidc_provider_requires_saved_endpoints() {
    let mut provider: super::OAuth2GenericConfig = serde_json::from_value(serde_json::json!({
        "id": "company", "display_name": "Company", "issuer": "https://id.example",
        "client_id": "pkgly", "client_secret": "secret"
    }))
    .unwrap();
    assert_eq!(
        provider.validate().unwrap_err(),
        "OIDC authorization, token, and signing key URLs are required"
    );
    provider.enabled = false;
    assert!(provider.validate().is_ok());
}

#[test]
fn generic_oidc_configuration_uses_generic_providers() {
    let source = r#"
[security.oauth2]
enabled = true

[[security.oauth2.generic_providers]]
id = "company"
display_name = "Company"
issuer = "https://id.example"
client_id = "pkgly"
client_secret = "fixture"

[security.oauth2.generic_providers.endpoints]
authorization_url = "https://id.example/authorize"
token_url = "https://id.example/token"
jwks_url = "https://id.example/keys"
"#;
    let document: toml::Value = toml::from_str(source).unwrap();
    let settings: super::OAuth2Settings =
        document["security"]["oauth2"].clone().try_into().unwrap();
    settings.validate_providers().unwrap();
    assert_eq!(settings.generic_providers[0].id, "company");
    let serialized = serde_json::to_value(&settings).unwrap();
    assert!(serialized.get("providers").is_none());
    assert_eq!(serialized["generic_providers"][0]["id"], "company");
    let encoded = toml::to_string(&settings).unwrap();
    assert!(encoded.contains("[[generic_providers]]"));
    assert!(encoded.contains("[generic_providers.endpoints]"));
}
