// ABOUTME: Verifies user identity value types reject invalid input with clear errors.
// ABOUTME: Keeps username and email validation behavior aligned with API responses.
#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn test_username() {
    let username = Username::new("test".to_string()).unwrap();
    assert_eq!(username.to_string(), "test");
    assert!(Username::new("te".to_string()).is_err());
    assert!(Username::new("testtesttesttesttesttesttesttesttest".to_string()).is_err());
    assert!(Username::new("test$".to_string()).is_err());
}

#[test]
fn invalid_email_length_errors_name_email() {
    let too_short = Email::new(String::new()).unwrap_err();
    assert_eq!(
        too_short.to_string(),
        "Email is too short, must be at least 3 got 0 characters"
    );

    let too_long = Email::new(format!("{}@example.com", "a".repeat(32))).unwrap_err();
    assert_eq!(
        too_long.to_string(),
        "Email is too long, must be less than 32 got 44 characters"
    );
}
