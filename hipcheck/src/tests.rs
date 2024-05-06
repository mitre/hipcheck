// SPDX-License-Identifier: Apache-2.0

//! Tests to ensure `Error` produces output correctly.

use crate::error::Error;
use crate::hc_error;
use std::io;
use std::io::ErrorKind;

// Message source root error with no context
#[test]
fn debug_behavior_msg_no_context() {
	let error = Error::msg("error message");
	let debug = format!("{:?}", error);
	let expected = "error message".to_string();
	assert_eq!(expected, debug);
}

// Message source root error with a single context message
#[test]
fn debug_behavior_msg_single_context() {
	let error = Error::msg("error message").context("context");
	let debug = format!("{:?}", error);
	let expected = "context\n\nCaused by: \n    0: error message".to_string();
	assert_eq!(expected, debug);
}

// Message source root error with multiple context messages
#[test]
fn debug_behavior_msg_multiple_context() {
	let error = Error::msg("error message")
		.context("context 1")
		.context("context 2");
	let debug = format!("{:?}", error);
	let expected = "context 2\n\nCaused by: \n    0: context 1\n    1: error message".to_string();
	assert_eq!(expected, debug);
}

// Dynamic error source with no context
#[test]
fn debug_behavior_std_no_context() {
	let error = Error::from(io::Error::new(
		ErrorKind::ConnectionRefused,
		"connection refused",
	));

	let debug = format!("{:?}", error);
	let expected = "connection refused".to_string();
	assert_eq!(expected, debug);
}

// Dynamic error source with a single context message
#[test]
fn debug_behavior_std_single_context() {
	let error = Error::from(io::Error::new(
		ErrorKind::ConnectionRefused,
		"connection refused",
	))
	.context("context");

	let debug = format!("{:?}", error);
	let expected = "context\n\nCaused by: \n    0: connection refused".to_string();
	assert_eq!(expected, debug);
}

// Dynamic error source with multiple context messages
#[test]
fn debug_behavior_std_multiple_context() {
	let error = Error::from(io::Error::new(
		ErrorKind::ConnectionRefused,
		"connection refused",
	))
	.context("context 1")
	.context("context 2");

	let debug = format!("{:?}", error);
	let expected =
		"context 2\n\nCaused by: \n    0: context 1\n    1: connection refused".to_string();
	assert_eq!(expected, debug);
}

// Literal input to `hc_error`
#[test]
fn macro_literal() {
	let error = hc_error!("msg source");
	let debug = format!("{:?}", error);
	let expected = "msg source".to_string();
	assert_eq!(expected, debug);
}

// Format string input to `hc_error`
#[test]
fn macro_format_string() {
	let msg = "msg";
	let source = "source";
	let error = hc_error!("format {} {}", msg, source);
	let debug = format!("{:?}", error);
	let expected = "format msg source".to_string();
	assert_eq!(expected, debug);
}

// Verify that the `chain` method on `hc_error` works.
#[test]
fn hc_error_chain() {
	let error = hc_error!("first error");
	let error = error.context("second error");
	let error = error.context("third error");

	let mut iter = error.chain();

	assert_eq!("third error", iter.next().unwrap().to_string());
	assert_eq!("second error", iter.next().unwrap().to_string());
	assert_eq!("first error", iter.next().unwrap().to_string());
}
