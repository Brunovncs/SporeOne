//! What SporeOne asks of Windows: registry values, a program's file version, and starting a
//! program the way Explorer would, with the administrator prompt when it asks for one.

use std::path::Path;

#[cfg(windows)]
mod imp {
    use crate::version::Version;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY,
        RegGetValueW,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};

    pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain([0]).collect()
    }

    #[derive(Clone, Copy)]
    pub enum Hive {
        User,
        Machine,
    }

    fn hkey(h: Hive) -> HKEY {
        match h {
            Hive::User => HKEY_CURRENT_USER,
            Hive::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    /// A string value, looked for in the 32-bit view first (where Spore's installers write) and
    /// then the 64-bit one.
    pub fn reg_string(hive: Hive, key: &str, value: &str) -> Option<String> {
        [RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY].into_iter().find_map(|view| {
            let (k, v) = (wide(key), wide(value));
            let mut buf = vec![0u16; 1024];
            let mut len = (buf.len() * 2) as u32;
            let ok = unsafe {
                RegGetValueW(
                    hkey(hive),
                    k.as_ptr(),
                    v.as_ptr(),
                    RRF_RT_REG_SZ | view,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut len,
                )
            };
            (ok == 0).then(|| {
                buf.truncate((len as usize / 2).saturating_sub(1));
                String::from_utf16_lossy(&buf)
            })
        })
    }

    pub fn reg_dword(hive: Hive, key: &str, value: &str) -> Option<u32> {
        let (k, v) = (wide(key), wide(value));
        let mut out = 0u32;
        let mut len = 4u32;
        let ok = unsafe {
            RegGetValueW(hkey(hive), k.as_ptr(), v.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&raw mut out).cast(), &mut len)
        };
        (ok == 0).then_some(out)
    }

    /// The four numbers of a program's file version, from its version resource.
    pub fn file_version(path: &Path) -> Option<Version> {
        let p = wide(path);
        let size = unsafe { GetFileVersionInfoSizeW(p.as_ptr(), std::ptr::null_mut()) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if unsafe { GetFileVersionInfoW(p.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
            return None;
        }
        let mut info: *mut VS_FIXEDFILEINFO = std::ptr::null_mut();
        let mut len = 0u32;
        let root = wide("\\");
        if unsafe { VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), (&raw mut info).cast(), &mut len) } == 0 || info.is_null() {
            return None;
        }
        let info = unsafe { &*info };
        let (ms, ls) = (info.dwFileVersionMS, info.dwFileVersionLS);
        Some(Version::new(ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff))
    }

    /// Starts `exe` as Explorer would: a program whose manifest asks for administrator rights
    /// gets Windows' prompt, which `Command` cannot give. `elevated` asks for it in any case. With
    /// `wait`, returns the exit code; `None` when the person said no to the prompt.
    pub fn shell_run(exe: &Path, args: &str, dir: Option<&Path>, elevated: bool, wait: bool) -> Result<Option<u32>, String> {
        let verb = wide(if elevated { "runas" } else { "open" });
        let file = wide(exe);
        let params = wide(args);
        let dir = dir.map(wide);
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        if let Some(d) = &dir {
            info.lpDirectory = d.as_ptr();
        }
        info.nShow = 1;
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let e = unsafe { GetLastError() };
            return if e == ERROR_CANCELLED { Ok(None) } else { Err(std::io::Error::from_raw_os_error(e as i32).to_string()) };
        }
        if info.hProcess.is_null() {
            return Ok(Some(0));
        }
        let mut code = 0u32;
        unsafe {
            if wait && WaitForSingleObject(info.hProcess, INFINITE) == WAIT_OBJECT_0 {
                GetExitCodeProcess(info.hProcess, &mut code);
            }
            CloseHandle(info.hProcess);
        }
        Ok(Some(code))
    }
}

#[cfg(windows)]
pub use imp::*;

/// Whether this process may create files in `dir`, tried rather than worked out from its
/// permissions: Program Files and the virtual store make the rules hard to read.
pub fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".sporeone-{}.tmp", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn file_version(_: &Path) -> Option<crate::version::Version> {
    None
}
