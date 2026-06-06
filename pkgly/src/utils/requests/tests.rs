// ABOUTME: Verifies request sanitization and typed JSON rejection responses.
// ABOUTME: Ensures invalid user fields produce actionable API error payloads.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use axum::response::IntoResponse;
use http::StatusCode;
use http_body_util::BodyExt;
use nr_core::database::entities::user::NewUserRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::json::JsonBody;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SomeThingThatTakesAnOptionString {
    #[serde(with = "crate::utils::serde_sanitize_string")]
    pub name: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeepTrimmed {
    #[serde(with = "crate::utils::serde_sanitize_string_keep_trimmed")]
    pub name: Option<String>,
}
#[test]
pub fn test_deserialize() {
    let json = r#"{"name": "  "}"#;
    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
}
#[test]
pub fn test_deserialize_null() {
    let json = r#"{"name": null}"#;
    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.name, None);
}

#[test]
pub fn test_serialize() {
    let thing = SomeThingThatTakesAnOptionString { name: None };
    let serialized = serde_json::to_string(&thing).unwrap();
    assert_eq!(serialized, r#"{"name":null}"#);
}

#[test]
pub fn test_serialize_some() {
    let thing = SomeThingThatTakesAnOptionString {
        name: Some("  ".to_owned()),
    };
    let serialized = serde_json::to_string(&thing).unwrap();
    assert_eq!(serialized, r#"{"name":null}"#);
}
#[test]
pub fn keeps_trimmed() {
    let json = r#"{"name": " some value "}"#;
    let deserialized: KeepTrimmed = serde_json::from_str(json).unwrap();

    assert_eq!(deserialized.name, Some("some value".to_owned()));

    let deserialized: SomeThingThatTakesAnOptionString = serde_json::from_str(json).unwrap();

    assert_eq!(deserialized.name, Some(" some value ".to_owned()));
}

#[tokio::test]
async fn invalid_user_email_response_names_email_error() {
    let rejection = JsonBody::<NewUserRequest>::from_bytes(
        br#"{"name":"Test","username":"test1","email":"","password":null}"#,
    )
    .unwrap_err();

    let response = rejection.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["message"],
        "Email is too short, must be at least 3 got 0 characters"
    );
    assert_eq!(payload["details"], "email");
}
