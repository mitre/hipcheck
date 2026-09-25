//! Process boundary for running Hipcheck checks.
//!
//! Report parsing deliberately remains outside this module. This boundary owns
//! only the untrusted process and returns bounded raw output.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use camino::Utf8PathBuf;
use thiserror::Error;
use tokio::io::AsyncReadExt as _;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

/// Night Vision-owned limits and process settings for one Hipcheck check.
#[derive(Clone, Debug)]
pub struct HipcheckRunnerConfig {
	/// Absolute path to the deployed Hipcheck binary.
	pub program: Utf8PathBuf,
	/// Night Vision-owned directory from which Hipcheck executes.
	pub working_directory: Utf8PathBuf,
	/// Absolute path to the Night Vision Hipcheck policy.
	pub policy_path: Utf8PathBuf,
	/// Absolute path to the Night Vision Hipcheck exec configuration.
	pub exec_config_path: Utf8PathBuf,
	/// Night Vision-owned writable Hipcheck cache directory.
	pub cache_directory: Utf8PathBuf,
	/// Complete allowlisted environment passed to Hipcheck and its plugins.
	pub environment: BTreeMap<OsString, OsString>,
	/// Maximum wall-clock time for the check.
	pub timeout: Duration,
	/// Maximum captured standard-output bytes.
	pub stdout_max_bytes: usize,
	/// Maximum captured standard-error bytes.
	pub stderr_max_bytes: usize,
	/// Maximum JSON report bytes emitted on standard output.
	pub json_max_bytes: usize,
}

/// Explicit, already-tokenized target arguments following the fixed hc check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HipcheckCheckRequest {
	/// Target arguments passed without shell interpretation.
	pub arguments: Vec<OsString>,
}

/// Bounded output from a Hipcheck process that completed within its limits.
#[derive(Debug)]
pub struct HipcheckExecutionOutput {
	/// Hipcheck's exit status, preserved for downstream failure mapping.
	pub status: ExitStatus,
	/// Raw bounded standard output.
	pub stdout: Vec<u8>,
	/// Raw bounded standard error.
	pub stderr: Vec<u8>,
	/// Raw bounded JSON report candidate bytes, including nonzero exits.
	///
	/// This layer deliberately does not parse the report. Downstream mapping
	/// decides whether a complete report can be trusted.
	pub json: Option<Vec<u8>>,
}

/// An output stream governed by a configured size limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HipcheckOutputStream {
	/// Standard output.
	Stdout,
	/// Standard error.
	Stderr,
	/// The JSON report on standard output.
	Json,
}

/// A process-boundary failure while running Hipcheck.
#[derive(Debug, Error)]
pub enum HipcheckExecutionError {
	/// Hipcheck could not be started.
	#[error("could not start Hipcheck")]
	Start(#[source] std::io::Error),
	/// A Hipcheck output stream could not be read.
	#[error("could not read Hipcheck {stream:?}")]
	Read {
		/// Stream whose read failed.
		stream: HipcheckOutputStream,
		/// Underlying I/O error.
		#[source]
		source: std::io::Error,
	},
	/// Hipcheck exceeded its wall-clock limit and was stopped.
	#[error("Hipcheck timed out after {} ms", timeout.as_millis())]
	TimedOut {
		/// Configured timeout.
		timeout: Duration,
		/// Bounded diagnostic output; never trusted as JSON.
		stdout: Vec<u8>,
		/// Bounded diagnostic output.
		stderr: Vec<u8>,
	},
	/// Hipcheck output exceeded a configured cap and was stopped.
	#[error("Hipcheck {stream:?} exceeded its {limit}-byte output limit")]
	OutputLimitExceeded {
		/// Stream that crossed the cap.
		stream: HipcheckOutputStream,
		/// Configured cap.
		limit: usize,
		/// Bounded diagnostic output.
		stdout: Vec<u8>,
		/// Bounded diagnostic output.
		stderr: Vec<u8>,
	},
	/// Waiting for Hipcheck failed.
	#[error("could not wait for Hipcheck")]
	Wait(#[source] std::io::Error),
	/// A reader task ended before returning its result.
	#[error("Hipcheck {stream:?} reader ended unexpectedly")]
	ReaderEnded {
		/// Stream whose reader ended.
		stream: HipcheckOutputStream,
	},
}

impl HipcheckExecutionError {
	/// Whether rerunning the same check may succeed.
	pub fn retryable(&self) -> bool {
		matches!(
			self,
			Self::Read { .. } | Self::TimedOut { .. } | Self::Wait(_) | Self::ReaderEnded { .. }
		)
	}
}

/// Run hc check through the Night Vision-owned process boundary.
pub async fn run_hipcheck_check(
	config: &HipcheckRunnerConfig,
	request: &HipcheckCheckRequest,
) -> Result<HipcheckExecutionOutput, HipcheckExecutionError> {
	validate_paths(config).map_err(HipcheckExecutionError::Start)?;
	let stdout_limit = config.stdout_max_bytes.min(config.json_max_bytes);
	let stderr_limit = config.stderr_max_bytes;
	let stdout_overflow = if config.stdout_max_bytes <= config.json_max_bytes {
		(HipcheckOutputStream::Stdout, config.stdout_max_bytes)
	} else {
		(HipcheckOutputStream::Json, config.json_max_bytes)
	};
	let mut command = Command::new(&config.program);
	command
		.arg("--policy")
		.arg(&config.policy_path)
		.arg("--exec")
		.arg(&config.exec_config_path)
		.arg("--cache")
		.arg(&config.cache_directory)
		.arg("check")
		.args(["--format", "json"])
		.args(["--verbosity", "quiet"])
		.args(&request.arguments)
		.current_dir(&config.working_directory)
		.env_clear()
		.envs(&config.environment)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.kill_on_drop(true);

	let mut child = command.spawn().map_err(HipcheckExecutionError::Start)?;
	let stdout = child.stdout.take().expect("stdout was piped");
	let stderr = child.stderr.take().expect("stderr was piped");
	let (stdout_sender, mut stdout_receiver) = oneshot::channel();
	let (stderr_sender, mut stderr_receiver) = oneshot::channel();
	tokio::spawn(async move {
		let _ = stdout_sender
			.send(read_bounded(stdout, stdout_limit, HipcheckOutputStream::Stdout).await);
	});
	tokio::spawn(async move {
		let _ = stderr_sender
			.send(read_bounded(stderr, stderr_limit, HipcheckOutputStream::Stderr).await);
	});

	let timeout = tokio::time::sleep(config.timeout);
	tokio::pin!(timeout);
	let mut stdout_output = None;
	let mut stderr_output = None;
	let status = loop {
		tokio::select! {
			status = child.wait() => break status.map_err(HipcheckExecutionError::Wait)?,
			result = &mut stdout_receiver, if stdout_output.is_none() => {
				let output = receive_result(result, HipcheckOutputStream::Stdout)?;
				if output.exceeded {
					stop_child(&mut child).await?;
					let stderr = receive_output(&mut stderr_receiver, HipcheckOutputStream::Stderr).await?;
					return Err(HipcheckExecutionError::OutputLimitExceeded {
						stream: stdout_overflow.0,
						limit: stdout_overflow.1,
						stdout: output.bytes,
						stderr: stderr.bytes,
					});
				}
				stdout_output = Some(output);
			}
			result = &mut stderr_receiver, if stderr_output.is_none() => {
				let output = receive_result(result, HipcheckOutputStream::Stderr)?;
				if output.exceeded {
					stop_child(&mut child).await?;
					let stdout = receive_output(&mut stdout_receiver, HipcheckOutputStream::Stdout).await?;
					return Err(HipcheckExecutionError::OutputLimitExceeded {
						stream: HipcheckOutputStream::Stderr,
						limit: config.stderr_max_bytes,
						stdout: stdout.bytes,
						stderr: output.bytes,
					});
				}
				stderr_output = Some(output);
			}
			() = &mut timeout => {
				stop_child(&mut child).await?;
				let stdout = match stdout_output {
					Some(output) => output,
					None => receive_output(&mut stdout_receiver, HipcheckOutputStream::Stdout).await?,
				};
				let stderr = match stderr_output {
					Some(output) => output,
					None => receive_output(&mut stderr_receiver, HipcheckOutputStream::Stderr).await?,
				};
				return Err(HipcheckExecutionError::TimedOut {
					timeout: config.timeout,
					stdout: stdout.bytes,
					stderr: stderr.bytes,
				});
			}
		}
	};

	let stdout = match stdout_output {
		Some(output) => output,
		None => receive_output(&mut stdout_receiver, HipcheckOutputStream::Stdout).await?,
	};
	let stderr = match stderr_output {
		Some(output) => output,
		None => receive_output(&mut stderr_receiver, HipcheckOutputStream::Stderr).await?,
	};
	if stdout.exceeded {
		return Err(HipcheckExecutionError::OutputLimitExceeded {
			stream: stdout_overflow.0,
			limit: stdout_overflow.1,
			stdout: stdout.bytes,
			stderr: stderr.bytes,
		});
	}
	if stderr.exceeded {
		return Err(HipcheckExecutionError::OutputLimitExceeded {
			stream: HipcheckOutputStream::Stderr,
			limit: config.stderr_max_bytes,
			stdout: stdout.bytes,
			stderr: stderr.bytes,
		});
	}
	let json = stdout
		.bytes
		.iter()
		.any(|byte| !byte.is_ascii_whitespace())
		.then(|| stdout.bytes.clone());
	Ok(HipcheckExecutionOutput {
		status,
		stdout: stdout.bytes,
		stderr: stderr.bytes,
		json,
	})
}

fn validate_paths(config: &HipcheckRunnerConfig) -> Result<(), std::io::Error> {
	for path in [
		&config.program,
		&config.policy_path,
		&config.exec_config_path,
	] {
		let path = resolve_path(&config.working_directory, path);
		if !path.is_file() {
			return Err(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				format!("Hipcheck artifact is not a file: {path}"),
			));
		}
	}
	for path in [&config.working_directory, &config.cache_directory] {
		let path = resolve_path(&config.working_directory, path);
		if !path.is_dir() {
			return Err(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				format!("Hipcheck runtime path is not a directory: {path}"),
			));
		}
	}
	Ok(())
}

fn resolve_path<'a>(working_directory: &'a Utf8PathBuf, path: &'a Utf8PathBuf) -> Utf8PathBuf {
	if path.is_absolute() {
		path.clone()
	} else {
		working_directory.join(path)
	}
}

struct BoundedOutput {
	bytes: Vec<u8>,
	exceeded: bool,
}

async fn read_bounded<R>(
	mut reader: R,
	limit: usize,
	stream: HipcheckOutputStream,
) -> Result<BoundedOutput, HipcheckExecutionError>
where
	R: tokio::io::AsyncRead + Unpin,
{
	let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
	loop {
		let remaining = limit.saturating_sub(bytes.len());
		let mut buffer = vec![0_u8; remaining.saturating_add(1).min(8 * 1024)];
		let read = reader
			.read(&mut buffer)
			.await
			.map_err(|source| HipcheckExecutionError::Read { stream, source })?;
		if read == 0 {
			return Ok(BoundedOutput {
				bytes,
				exceeded: false,
			});
		}
		let kept = read.min(remaining);
		bytes.extend_from_slice(&buffer[..kept]);
		if read > remaining {
			return Ok(BoundedOutput {
				bytes,
				exceeded: true,
			});
		}
	}
}

fn receive_result(
	result: Result<Result<BoundedOutput, HipcheckExecutionError>, oneshot::error::RecvError>,
	stream: HipcheckOutputStream,
) -> Result<BoundedOutput, HipcheckExecutionError> {
	result.map_err(|_| HipcheckExecutionError::ReaderEnded { stream })?
}

async fn receive_output(
	receiver: &mut oneshot::Receiver<Result<BoundedOutput, HipcheckExecutionError>>,
	stream: HipcheckOutputStream,
) -> Result<BoundedOutput, HipcheckExecutionError> {
	receive_result(receiver.await, stream)
}

async fn stop_child(child: &mut Child) -> Result<(), HipcheckExecutionError> {
	let _ = child.start_kill();
	child.wait().await.map_err(HipcheckExecutionError::Wait)?;
	Ok(())
}

#[cfg(all(test, unix))]
mod tests {
	use super::*;
	use std::fs;
	use std::os::unix::fs::PermissionsExt as _;
	use std::path::PathBuf;
	use std::sync::atomic::{AtomicUsize, Ordering};

	static TEST_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

	#[test]
	fn passes_literal_arguments_and_controlled_environment() {
		run_async(async {
			let fixture = TestFixture::new(
				r#"printf '%s\n' "$PWD" > "$NV_CAPTURE"
printf '%s\n' "$NV_ALLOWED" >> "$NV_CAPTURE"
printf '%s\n' "$PATH" >> "$NV_CAPTURE"
if test -n "$NV_INHERITED"; then exit 21; fi
printf '%s\n' "$@" >> "$NV_CAPTURE"
printf '{"report":"ok"}'
"#,
			);
			let request = HipcheckCheckRequest {
				arguments: vec![OsString::from("; touch should-not-run")],
			};
			let output = run_hipcheck_check(&fixture.config(), &request)
				.await
				.expect("check should succeed");

			assert!(output.status.success());
			assert_eq!(output.stdout, b"{\"report\":\"ok\"}");
			assert_eq!(
				output.json.as_deref(),
				Some(b"{\"report\":\"ok\"}".as_slice())
			);
			let capture = fs::read_to_string(fixture.capture_path()).expect("capture should exist");
			let mut captured_lines = capture.lines();
			assert_eq!(
				std::fs::canonicalize(
					captured_lines
						.next()
						.expect("working directory should be captured")
				)
				.expect("captured working directory should exist"),
				std::fs::canonicalize(&fixture.working_directory)
					.expect("configured working directory should exist")
			);
			assert_eq!(
				captured_lines.collect::<Vec<_>>(),
				[
					"allowed",
					"/allowed/bin",
					"--policy",
					"policy file.hc",
					"--exec",
					"exec file.hc",
					"--cache",
					"cache directory",
					"check",
					"--format",
					"json",
					"--verbosity",
					"quiet",
					"; touch should-not-run"
				]
			);
			assert!(!fixture.working_directory.join("should-not-run").exists());
		});
	}

	#[test]
	fn nonzero_exit_preserves_complete_json_candidate() {
		run_async(async {
			let fixture = TestFixture::new("printf '{\"recommendation\":\"INVESTIGATE\"}'\nexit 7");
			let output = run_hipcheck_check(
				&fixture.config(),
				&HipcheckCheckRequest { arguments: vec![] },
			)
			.await
			.expect("a completed process should return bounded output");

			assert!(!output.status.success());
			assert_eq!(output.status.code(), Some(7));
			assert_eq!(output.json.as_deref(), Some(output.stdout.as_slice()));
		});
	}

	#[test]
	fn timeout_stops_process_and_marks_failure_retryable() {
		run_async(async {
			let fixture = TestFixture::new(
				r#"printf '%s' "$$" > "$NV_PID"
while test ! -f "$NV_RELEASE"; do :; done
printf '{"partial":'
while :; do :; done"#,
			);
			let mut config = fixture.config();
			config.timeout = Duration::from_secs(1);
			let run = tokio::spawn(async move {
				run_hipcheck_check(&config, &HipcheckCheckRequest { arguments: vec![] }).await
			});
			wait_for_file(&fixture.process_id_path()).await;
			fs::write(fixture.release_path(), "release").expect("release signal should write");
			let error = run
				.await
				.expect("runner task should not panic")
				.expect_err("check should time out");

			assert!(error.retryable());
			let HipcheckExecutionError::TimedOut { stdout, .. } = error else {
				panic!("check should time out");
			};
			assert_eq!(stdout, b"{\"partial\":");
			assert!(!process_exists(fixture.process_id()));
		});
	}

	#[test]
	fn bounds_stdout_stderr_and_json() {
		run_async(async {
			let cases = [
				("printf 12345", "", HipcheckOutputStream::Stdout, 4, 32, 32),
				(
					"",
					"printf 12345 >&2",
					HipcheckOutputStream::Stderr,
					32,
					4,
					32,
				),
				("printf 12345", "", HipcheckOutputStream::Json, 32, 32, 4),
			];
			for (stdout, stderr, expected_stream, stdout_limit, stderr_limit, json_limit) in cases {
				let fixture = TestFixture::new(&format!("{stdout}\n{stderr}\nwhile :; do :; done"));
				let mut config = fixture.config();
				config.stdout_max_bytes = stdout_limit;
				config.stderr_max_bytes = stderr_limit;
				config.json_max_bytes = json_limit;
				let error =
					run_hipcheck_check(&config, &HipcheckCheckRequest { arguments: vec![] })
						.await
						.expect_err("oversized output should fail");
				match error {
					HipcheckExecutionError::OutputLimitExceeded { stream, limit, .. } => {
						assert_eq!(stream, expected_stream);
						assert_eq!(
							limit,
							match expected_stream {
								HipcheckOutputStream::Stdout => stdout_limit,
								HipcheckOutputStream::Stderr => stderr_limit,
								HipcheckOutputStream::Json => json_limit,
							}
						);
					}
					other => panic!("expected output-limit error, got {other:?}"),
				}
			}
		});
	}

	fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.expect("test runtime should build")
			.block_on(future)
	}

	struct TestFixture {
		root: PathBuf,
		working_directory: PathBuf,
	}

	impl TestFixture {
		fn new(script: &str) -> Self {
			let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
			let root = std::env::temp_dir().join(format!(
				"nv-hipcheck-runner-{}-{sequence}",
				std::process::id()
			));
			let working_directory = root.join("working");
			fs::create_dir_all(&working_directory).expect("test directory should create");
			fs::write(working_directory.join("policy file.hc"), "policy")
				.expect("test policy should write");
			fs::write(working_directory.join("exec file.hc"), "exec")
				.expect("test exec config should write");
			fs::create_dir(working_directory.join("cache directory"))
				.expect("test cache directory should create");
			let program = root.join("fake-hc");
			fs::write(&program, format!("#!/bin/sh\n{script}\n"))
				.expect("test program should write");
			let mut permissions = fs::metadata(&program)
				.expect("test program metadata should read")
				.permissions();
			permissions.set_mode(0o700);
			fs::set_permissions(&program, permissions).expect("test program should be executable");
			Self {
				root,
				working_directory,
			}
		}

		fn capture_path(&self) -> PathBuf {
			self.root.join("capture")
		}

		fn process_id(&self) -> String {
			fs::read_to_string(self.process_id_path())
				.expect("process ID should be captured")
				.trim()
				.to_owned()
		}

		fn process_id_path(&self) -> PathBuf {
			self.root.join("pid")
		}

		fn release_path(&self) -> PathBuf {
			self.root.join("release")
		}

		fn config(&self) -> HipcheckRunnerConfig {
			let mut environment = BTreeMap::new();
			environment.insert(OsString::from("NV_ALLOWED"), OsString::from("allowed"));
			environment.insert(OsString::from("PATH"), OsString::from("/allowed/bin"));
			environment.insert(
				OsString::from("NV_CAPTURE"),
				self.capture_path().into_os_string(),
			);
			environment.insert(
				OsString::from("NV_PID"),
				self.process_id_path().into_os_string(),
			);
			environment.insert(
				OsString::from("NV_RELEASE"),
				self.release_path().into_os_string(),
			);
			HipcheckRunnerConfig {
				program: Utf8PathBuf::from_path_buf(self.root.join("fake-hc"))
					.expect("test program path should be UTF-8"),
				working_directory: Utf8PathBuf::from_path_buf(self.working_directory.clone())
					.expect("test working directory should be UTF-8"),
				policy_path: Utf8PathBuf::from("policy file.hc"),
				exec_config_path: Utf8PathBuf::from("exec file.hc"),
				cache_directory: Utf8PathBuf::from("cache directory"),
				environment,
				timeout: Duration::from_secs(2),
				stdout_max_bytes: 1024,
				stderr_max_bytes: 1024,
				json_max_bytes: 1024,
			}
		}
	}

	impl Drop for TestFixture {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.root);
		}
	}

	async fn wait_for_file(path: &std::path::Path) {
		tokio::time::timeout(Duration::from_secs(1), async {
			while !path.exists() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		})
		.await
		.expect("fake Hipcheck process should signal that it started");
	}

	fn process_exists(process_id: String) -> bool {
		std::process::Command::new("kill")
			.args(["-0", &process_id])
			.status()
			.expect("process liveness check should run")
			.success()
	}
}
