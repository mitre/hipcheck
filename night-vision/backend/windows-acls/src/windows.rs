use crate::FilePermissionsExt;
use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt as _, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, GENERIC_READ, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACL,
        Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, IsWellKnownSid, PSECURITY_DESCRIPTOR,
        WinAuthenticatedUserSid, WinBuiltinGuestsSid, WinBuiltinUsersSid, WinWorldSid,
    },
    Storage::FileSystem::FILE_GENERIC_READ,
};

impl FilePermissionsExt for Path {
    fn allows_broad_read(&self) -> io::Result<bool> {
        let mut dacl = ptr::null_mut();
        let mut security_descriptor = ptr::null_mut();
        let path = windows_path(self.as_os_str());

        // SAFETY: `path` is a null-terminated UTF-16 path. The remaining out pointers are either
        // null for fields we do not request or valid local variables. On success, Windows
        // allocates the security descriptor and `SecurityDescriptor` releases it with `LocalFree`.
        let result = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &raw mut dacl,
                ptr::null_mut(),
                &raw mut security_descriptor,
            )
        };

        if result != ERROR_SUCCESS {
            return Err(windows_error(result));
        }

        let _security_descriptor = SecurityDescriptor(security_descriptor);
        // SAFETY: `dacl` came from `GetNamedSecurityInfoW` and remains valid while
        // `_security_descriptor` is alive.
        Ok(dacl.is_null() || unsafe { dacl_allows_broad_read(dacl) }?)
    }
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: The pointer came from `GetNamedSecurityInfoW`, which documents that callers
            // must release the returned security descriptor with `LocalFree`.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

/// Returns whether `dacl` grants read access to a broad Windows principal.
///
/// # Safety
///
/// `dacl` must point to a valid Windows ACL for the duration of the call.
unsafe fn dacl_allows_broad_read(dacl: *mut ACL) -> io::Result<bool> {
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const BROAD_READ_ACCESS: u32 = FILE_GENERIC_READ | GENERIC_READ;

    for ace_index in 0..u32::from(unsafe { (*dacl).AceCount }) {
        let mut ace = ptr::null_mut();

        // SAFETY: The caller guarantees that `dacl` points to a valid Windows ACL.
        if unsafe { GetAce(dacl, ace_index, &raw mut ace) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
        if unsafe { (*ace).Header.AceType } != ACCESS_ALLOWED_ACE_TYPE {
            continue;
        }

        if unsafe { (*ace).Mask } & BROAD_READ_ACCESS == 0 {
            continue;
        }

        let sid = unsafe { ptr::addr_of!((*ace).SidStart) }
            .cast::<std::ffi::c_void>()
            .cast_mut();
        // SAFETY: `sid` points to the SID stored in an ACE returned by `GetAce`, and the ACE
        // remains valid while `dacl` is valid.
        if unsafe { is_broad_windows_principal(sid) } {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Returns whether `sid` is one of the broad Windows principals rejected for secret files.
///
/// # Safety
///
/// `sid` must point to a valid Windows SID for the duration of the call.
unsafe fn is_broad_windows_principal(sid: *mut std::ffi::c_void) -> bool {
    [
        WinWorldSid,
        WinAuthenticatedUserSid,
        WinBuiltinUsersSid,
        WinBuiltinGuestsSid,
    ]
    .into_iter()
    .any(|well_known_sid| {
        // SAFETY: The caller guarantees that `sid` points to a valid Windows SID.
        unsafe { IsWellKnownSid(sid, well_known_sid) != 0 }
    })
}

/// Converts a platform path into the null-terminated UTF-16 form expected by Windows APIs.
fn windows_path(path: &OsStr) -> Vec<u16> {
    path.encode_wide().chain(std::iter::once(0)).collect()
}

/// Converts a Windows error code into a standard I/O error.
fn windows_error(code: u32) -> io::Error {
    let code = i32::try_from(code).unwrap_or(i32::MAX);
    io::Error::from_raw_os_error(code)
}
