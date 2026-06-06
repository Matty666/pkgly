// ABOUTME: Verifies shared API response payloads preserve actionable error context.
// ABOUTME: Covers conflict responses consumed by user-management frontend flows.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

use axum::response::IntoResponse;
use http::StatusCode;
use http_body_util::BodyExt;
use serde_json::Value;

use super::conflict::ConflictResponse;

#[tokio::test]
async fn conflict_response_names_duplicate_field() {
    let response = ConflictResponse::from("username").into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["message"], "username already exists");
    assert_eq!(payload["details"], "username");
}
