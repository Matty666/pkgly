use super::*;
use crate::app::config::{OAuth2GenericConfig, OAuth2GenericEndpoints, validate_oidc_url};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde_json::{Map, json};
use sha2::{Digest, Sha256};

pub(super) async fn runtime(
    config: &OAuth2GenericConfig,
    settings: &OAuth2Settings,
    client: &HttpClient,
) -> Result<OAuth2ProviderRuntime, String> {
    let endpoints = match &config.endpoints {
        Some(endpoints) => endpoints.clone(),
        None => discover(config, client).await?,
    };
    let auth_type = match config.token_endpoint_auth_method.as_str() {
        "client_secret_basic" => oauth2::AuthType::BasicAuth,
        "client_secret_post" => oauth2::AuthType::RequestBody,
        _ => return Err("Unsupported OIDC client authentication method".into()),
    };
    let client = OAuthClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_type(auth_type)
        .set_auth_uri(
            AuthUrl::new(endpoints.authorization_url).map_err(|_| "Invalid authorization URL")?,
        )
        .set_token_uri(TokenUrl::new(endpoints.token_url).map_err(|_| "Invalid token URL")?);
    Ok(OAuth2ProviderRuntime {
        provider: OAuth2ProviderKind::Custom(config.id.clone()),
        client,
        scopes: normalize_scopes(&config.scopes),
        redirect_path: config
            .redirect_path
            .clone()
            .unwrap_or_else(|| settings.callback_path.clone()),
        display_name: config.display_name.clone(),
        generic: Some(config.clone()),
        jwks_url: Some(endpoints.jwks_url),
    })
}

async fn discover(
    config: &OAuth2GenericConfig,
    client: &HttpClient,
) -> Result<OAuth2GenericEndpoints, String> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        config.issuer.trim_end_matches('/')
    );
    let response = crate::utils::upstream::send(client, client.get(url))
        .await
        .map_err(|_| "OIDC discovery request failed")?;
    if !response.status().is_success() {
        return Err("OIDC discovery returned an error status".into());
    }
    let metadata = response
        .json::<Value>()
        .await
        .map_err(|_| "Invalid OIDC discovery metadata")?;
    resolve_metadata(config, metadata)
}

pub(super) fn resolve_metadata(
    config: &OAuth2GenericConfig,
    metadata: Value,
) -> Result<OAuth2GenericEndpoints, String> {
    if metadata.get("issuer").and_then(Value::as_str) != Some(config.issuer.as_str()) {
        return Err("OIDC discovery issuer mismatch".into());
    }
    validate_metadata_support(config, &metadata)?;
    let endpoint = |field: &str| -> Result<String, String> {
        let value = metadata[field]
            .as_str()
            .ok_or_else(|| format!("OIDC discovery missing {field}"))?;
        validate_oidc_url(value)?;
        Ok(value.into())
    };
    Ok(OAuth2GenericEndpoints {
        authorization_url: endpoint("authorization_endpoint")?,
        token_url: endpoint("token_endpoint")?,
        jwks_url: endpoint("jwks_uri")?,
    })
}

fn validate_metadata_support(config: &OAuth2GenericConfig, metadata: &Value) -> Result<(), String> {
    for (field, required) in [
        ("response_types_supported", "code"),
        ("id_token_signing_alg_values_supported", "RS256"),
    ] {
        if !metadata[field]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(required)))
        {
            return Err(format!("OIDC discovery does not support {required}"));
        }
    }
    for (field, required) in [
        ("grant_types_supported", "authorization_code"),
        ("code_challenge_methods_supported", "S256"),
    ] {
        if metadata.get(field).is_some_and(|items| {
            !items
                .as_array()
                .is_some_and(|items| items.iter().any(|v| v.as_str() == Some(required)))
        }) {
            return Err(format!("OIDC discovery does not support {required}"));
        }
    }
    let methods = metadata
        .get("token_endpoint_auth_methods_supported")
        .cloned()
        .unwrap_or_else(|| json!(["client_secret_basic"]));
    if !methods.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|v| v.as_str() == Some(config.token_endpoint_auth_method.as_str()))
    }) {
        return Err("OIDC discovery client authentication mismatch".into());
    }
    Ok(())
}

pub(super) fn fingerprint(
    config: &OAuth2GenericConfig,
    callback_uri: &str,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(config).map_err(|_| "Cannot encode OIDC configuration")?;
    let mut hash = Sha256::new();
    hash.update(encoded);
    hash.update(callback_uri);
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    Ok(URL_SAFE_NO_PAD.encode(hash.finalize()))
}

pub(super) fn verify_claims(
    token: &str,
    key: &DecodingKey,
    config: &OAuth2GenericConfig,
    nonce: &str,
) -> Result<Map<String, Value>, String> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[config.issuer.as_str()]);
    validation.set_audience(&[config.client_id.as_str()]);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "sub", "iat"]);
    validation.leeway = 30;
    let claims = decode::<Map<String, Value>>(token, key, &validation)
        .map_err(|_| "Invalid OIDC identity token")?
        .claims;
    validate_identity_claims(&claims, config, nonce)?;
    Ok(claims)
}

fn validate_identity_claims(
    claims: &Map<String, Value>,
    config: &OAuth2GenericConfig,
    nonce: &str,
) -> Result<(), String> {
    let audience_matches = match claims.get("aud") {
        Some(Value::String(aud)) => aud == &config.client_id,
        Some(Value::Array(aud)) => {
            aud.len() == 1 && aud[0].as_str() == Some(config.client_id.as_str())
        }
        _ => false,
    };
    if !audience_matches
        || claims.get("nonce").and_then(Value::as_str) != Some(nonce)
        || !claims
            .get("sub")
            .and_then(Value::as_str)
            .is_some_and(|sub| !sub.is_empty())
        || !claims
            .get("iat")
            .and_then(Value::as_i64)
            .is_some_and(|iat| iat <= chrono::Utc::now().timestamp() + 30)
        || claims
            .get("azp")
            .is_some_and(|azp| azp.as_str() != Some(config.client_id.as_str()))
    {
        return Err("Invalid OIDC identity claims".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
