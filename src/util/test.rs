// SPDX-License-Identifier: Apache-2.0

//! Utilities for unit testing.

use std::{
	env,
	env::VarError,
	panic,
	panic::{RefUnwindSafe, UnwindSafe},
	sync::{Mutex, OnceLock},
};

static SERIAL_TEST: OnceLock<Mutex<()>> = OnceLock::new();

/// Sets environment variables to the given value for the duration of the closure.
/// Restores the previous values when the closure completes or panics,
/// before unwinding the panic.
///
/// Credit: Fabian Braun at https://stackoverflow.com/a/67433684
pub fn with_env_vars<F>(kvs: Vec<(&str, Option<&str>)>, closure: F)
where
	F: Fn() + UnwindSafe + RefUnwindSafe,
{
	let guard = SERIAL_TEST.get_or_init(Default::default).lock().unwrap();
	let mut old_kvs: Vec<(&str, Result<String, VarError>)> = Vec::new();
	for (k, v) in kvs {
		let old_v = env::var(k);
		old_kvs.push((k, old_v));
		match v {
			None => unsafe { env::remove_var(k) },
			Some(v) => unsafe { env::set_var(k, v) },
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
		Ok(v) => unsafe { env::set_var(k, v) },
		Err(NotUnicode(os_str)) => unsafe { env::set_var(k, os_str) },
		Err(NotPresent) => unsafe { env::remove_var(k) },
	}
}
