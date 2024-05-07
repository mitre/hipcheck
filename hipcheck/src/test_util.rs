// SPDX-License-Identifier: Apache-2.0

//! Utilities for unit testing.

use std::env;
use std::env::VarError;
use std::panic;
use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use std::sync::Mutex;

use lazy_static::lazy_static;

lazy_static! {
	static ref SERIAL_TEST: Mutex<()> = Default::default();
}

/// Sets environment variables to the given value for the duration of the closure.
/// Restores the previous values when the closure completes or panics,
/// before unwinding the panic.
///
/// Credit: Fabian Braun at https://stackoverflow.com/a/67433684
#[allow(unused)]
pub fn with_env_vars<F>(kvs: Vec<(&str, Option<&str>)>, closure: F)
where
	F: Fn() + UnwindSafe + RefUnwindSafe,
{
	let guard = SERIAL_TEST.lock().unwrap();
	let mut old_kvs: Vec<(&str, Result<String, VarError>)> = Vec::new();
	for (k, v) in kvs {
		let old_v = env::var(k);
		old_kvs.push((k, old_v));
		match v {
			None => env::remove_var(k),
			Some(v) => env::set_var(k, v),
		}
	}

	let scrutinee = panic::catch_unwind(|| {
		closure();
	});

	match scrutinee {
		Ok(_) => {
			for (k, v) in old_kvs {
				reset_env(k, v);
			}
		}
		Err(err) => {
			for (k, v) in old_kvs {
				reset_env(k, v);
			}
			drop(guard);
			panic::resume_unwind(err);
		}
	};
}

fn reset_env(k: &str, old: Result<String, VarError>) {
	use std::env::VarError::*;
	match old {
		Ok(v) => env::set_var(k, v),
		Err(NotUnicode(os_str)) => env::set_var(k, os_str),
		Err(NotPresent) => env::remove_var(k),
	}
}
