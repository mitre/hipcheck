use camino::Utf8Path;
use std::sync::{LazyLock, Mutex};

#[cfg(unix)]
use std::fs;
#[cfg(windows)]
use std::process::Command;

static INTEGRATION_TEST_DB_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn with_integration_test_db_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = INTEGRATION_TEST_DB_LOCK
        .lock()
        .expect("integration test DB lock should not be poisoned");
    operation()
}

pub enum TestFilePermissions {
    #[cfg(unix)]
    UnixOwnerOnly,
    #[cfg(unix)]
    UnixGroupOrWorldReadable,
    #[cfg(windows)]
    WindowsOwnerOnly,
    #[cfg(not(any(unix, windows)))]
    UncheckedOwnerOnly,
}

#[cfg(unix)]
pub fn set_file_permissions(path: &Utf8Path, permissions: TestFilePermissions) {
    let mode = match permissions {
        TestFilePermissions::UnixOwnerOnly => 0o600,
        TestFilePermissions::UnixGroupOrWorldReadable => 0o644,
    };
    let permissions = std::os::unix::fs::PermissionsExt::from_mode(mode);
    fs::set_permissions(path, permissions).expect("failed to set test file permissions");
}

#[cfg(windows)]
pub fn set_file_permissions(path: &Utf8Path, permissions: TestFilePermissions) {
    match permissions {
        TestFilePermissions::WindowsOwnerOnly => restrict_windows_file_permissions(path),
    }
}

#[cfg(not(any(unix, windows)))]
pub fn set_file_permissions(_path: &Utf8Path, permissions: TestFilePermissions) {
    match permissions {
        TestFilePermissions::UncheckedOwnerOnly => {}
    }
}

#[cfg(unix)]
pub fn restrict_secret_file_permissions(path: &Utf8Path) {
    set_file_permissions(path, TestFilePermissions::UnixOwnerOnly);
}

#[cfg(windows)]
pub fn restrict_secret_file_permissions(path: &Utf8Path) {
    set_file_permissions(path, TestFilePermissions::WindowsOwnerOnly);
}

#[cfg(not(any(unix, windows)))]
pub fn restrict_secret_file_permissions(path: &Utf8Path) {
    set_file_permissions(path, TestFilePermissions::UncheckedOwnerOnly);
}

#[cfg(windows)]
fn restrict_windows_file_permissions(path: &Utf8Path) {
    let account = current_windows_account();
    let grant = format!("{account}:F");
    let status = Command::new("icacls")
        .arg(path.as_std_path())
        .args(["/inheritance:r", "/grant:r", grant.as_str()])
        .status()
        .expect("failed to restrict test file ACL");
    assert!(status.success(), "failed to restrict test file ACL");
}

#[cfg(windows)]
fn current_windows_account() -> String {
    let output = Command::new("whoami")
        .output()
        .expect("failed to determine current Windows account");
    assert!(
        output.status.success(),
        "failed to determine current Windows account"
    );
    String::from_utf8(output.stdout)
        .expect("current Windows account should be UTF-8")
        .trim()
        .to_owned()
}
