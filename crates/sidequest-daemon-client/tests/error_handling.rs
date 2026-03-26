//! RED phase tests: DaemonError variants, Display impl, trait bounds.

use std::time::Duration;

use sidequest_daemon_client::DaemonError;

// ---------------------------------------------------------------------------
// Display messages — each variant must produce a specific, useful message
// ---------------------------------------------------------------------------

#[test]
fn socket_error_display_contains_io_detail() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
    let err = DaemonError::SocketError(io_err);
    let msg = err.to_string();
    assert!(
        msg.contains("connection refused"),
        "SocketError Display must include the IO error detail, got: {msg}"
    );
}

#[test]
fn timeout_display_contains_duration() {
    let err = DaemonError::Timeout {
        duration: Duration::from_secs(30),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("30"),
        "Timeout Display must include the duration value, got: {msg}"
    );
}

#[test]
fn timeout_display_indicates_timeout() {
    let err = DaemonError::Timeout {
        duration: Duration::from_secs(10),
    };
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("timeout"),
        "Timeout Display must mention 'timeout', got: {msg}"
    );
}

#[test]
fn invalid_response_display_contains_detail() {
    let err = DaemonError::InvalidResponse("missing 'result' field".into());
    let msg = err.to_string();
    assert!(
        msg.contains("missing 'result' field"),
        "InvalidResponse Display must include the detail string, got: {msg}"
    );
}

#[test]
fn daemon_error_response_display_contains_code() {
    let err = DaemonError::DaemonErrorResponse {
        code: -32000,
        message: "GPU out of memory".into(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("-32000"),
        "DaemonErrorResponse Display must include error code, got: {msg}"
    );
}

#[test]
fn daemon_error_response_display_contains_message() {
    let err = DaemonError::DaemonErrorResponse {
        code: -32000,
        message: "GPU out of memory".into(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("GPU out of memory"),
        "DaemonErrorResponse Display must include error message, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Trait bounds — required for async error propagation
// ---------------------------------------------------------------------------

#[test]
fn daemon_error_implements_std_error() {
    fn assert_error<T: std::error::Error>() {}
    assert_error::<DaemonError>();
}

#[test]
fn daemon_error_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<DaemonError>();
}

#[test]
fn daemon_error_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<DaemonError>();
}

// ---------------------------------------------------------------------------
// From conversions
// ---------------------------------------------------------------------------

#[test]
fn from_io_error_produces_socket_error_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "socket not found");
    let daemon_err = DaemonError::from(io_err);
    assert!(
        matches!(daemon_err, DaemonError::SocketError(_)),
        "From<io::Error> must produce DaemonError::SocketError"
    );
}

// ---------------------------------------------------------------------------
// Pattern matching — DaemonError must support exhaustive matching on known
// variants with a wildcard arm (enforced by #[non_exhaustive])
// ---------------------------------------------------------------------------

#[test]
fn daemon_error_variants_are_matchable() {
    let errors: Vec<DaemonError> = vec![
        DaemonError::SocketError(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        DaemonError::Timeout {
            duration: Duration::from_secs(5),
        },
        DaemonError::InvalidResponse("bad".into()),
        DaemonError::DaemonErrorResponse {
            code: -1,
            message: "fail".into(),
        },
    ];

    for err in &errors {
        // Every variant must be constructable and matchable.
        // The wildcard arm is required by #[non_exhaustive] for external crates.
        let label = match err {
            DaemonError::SocketError(_) => "socket",
            DaemonError::Timeout { .. } => "timeout",
            DaemonError::InvalidResponse(_) => "invalid",
            DaemonError::DaemonErrorResponse { .. } => "daemon",
            _ => "unknown",
        };
        assert_ne!(label, "unknown", "all known variants must match");
    }
}

// ---------------------------------------------------------------------------
// Each variant's Display message must be distinct
// ---------------------------------------------------------------------------

#[test]
fn each_variant_has_distinct_display_message() {
    let errors: Vec<DaemonError> = vec![
        DaemonError::SocketError(std::io::Error::new(std::io::ErrorKind::Other, "io problem")),
        DaemonError::Timeout {
            duration: Duration::from_secs(5),
        },
        DaemonError::InvalidResponse("parse problem".into()),
        DaemonError::DaemonErrorResponse {
            code: -1,
            message: "daemon problem".into(),
        },
    ];

    let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();

    // Each pair of messages must differ — a blanket "daemon error" for all is wrong.
    for i in 0..messages.len() {
        for j in (i + 1)..messages.len() {
            assert_ne!(
                messages[i], messages[j],
                "variant {} and {} must have different Display messages: both are '{}'",
                i, j, messages[i]
            );
        }
    }
}
