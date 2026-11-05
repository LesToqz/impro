//! The few Win32 and GDK-Win32 calls the native frame needs, wrapped in small safe functions.
//! Every wrapper tolerates failure by returning `None` or `false`; none of them panics.
use glib::translate::ToGlibPtr;
use gtk::prelude::*;
use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, HWND},
    Graphics::Dwm::DwmSetWindowAttribute,
    System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    },
    UI::{
        Shell::SetCurrentProcessExplicitAppUserModelID,
        WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION},
    },
};

extern "C" {
    /// gdk/win32/gdkwin32window.h. Exported by libgdk-3, which gdk-sys already links; the
    /// bindings crate has no win32 module, so it is declared here. The handle type is
    /// `HGDIOBJ` in C, a plain pointer that is a valid `HWND` for a toplevel `GdkWindow`.
    fn gdk_win32_window_get_handle(window: *mut gdk::ffi::GdkWindow) -> HWND;
}

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

/// Native handle of a realized toplevel; `None` before realize or if GDK has none.
pub fn hwnd(widget: &impl IsA<gtk::Widget>) -> Option<HWND> {
    let window = widget.as_ref().window()?;
    // SAFETY: `window` is a live GdkWindow of the Win32 backend (the only backend on Windows)
    // and the stash keeps it alive for the call; the function just reads its handle.
    let handle = unsafe { gdk_win32_window_get_handle(window.to_glib_none().0) };
    (!handle.is_null()).then_some(handle)
}

/// Set one 4-byte DWM window attribute. `false` when the attribute is unsupported on this
/// Windows version or the call fails; callers treat that as "not available", never an error.
pub fn set_window_attribute(hwnd: HWND, attribute: i32, value: u32) -> bool {
    if hwnd.is_null() {
        return false;
    }
    // SAFETY: `value` is a live 4-byte object for the duration of the call, which copies it;
    // `hwnd` is a window handle that DWM validates itself and rejects with an HRESULT if stale.
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute as u32,
            std::ptr::from_ref(&value).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    result >= 0
}

/// Make the shell group this process under `id` (taskbar pinning, jump lists, notifications).
pub fn set_app_user_model_id(id: &str) -> bool {
    let id = wide(id);
    // SAFETY: `id` is NUL-terminated and outlives the call; the shell copies the string.
    unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) >= 0 }
}

/// `AppsUseLightTheme`: `0` means dark apps, anything else light apps.
pub fn apps_use_light_theme() -> Option<bool> {
    registry_dword(HKEY_CURRENT_USER, PERSONALIZE_KEY, "AppsUseLightTheme").map(|v| v != 0)
}

/// Windows build number (for example `22631`), from `CurrentBuildNumber`.
pub fn build_number() -> Option<u32> {
    registry_string(HKEY_LOCAL_MACHINE, VERSION_KEY, "CurrentBuildNumber")
        .and_then(|text| crate::win::policy::parse_build_number(&text))
}

/// `SPI_GETCLIENTAREAANIMATION`: `Some(false)` when animations are switched off.
pub fn client_area_animations() -> Option<bool> {
    let mut enabled: i32 = 1;
    // SAFETY: this action writes exactly one BOOL (i32) through pvParam; `enabled` is one.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            std::ptr::from_mut(&mut enabled).cast(),
            0,
        )
    };
    (ok != 0).then_some(enabled != 0)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_dword(root: HKEY, subkey: &str, name: &str) -> Option<u32> {
    let (subkey, name) = (wide(subkey), wide(name));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names are NUL-terminated and outlive the call; `data` is a 4-byte buffer
    // whose length is passed in `size`; the type filter limits the result to REG_DWORD.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut data).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

fn registry_string(root: HKEY, subkey: &str, name: &str) -> Option<String> {
    let (subkey, name) = (wide(subkey), wide(name));
    let mut buffer = [0u16; 64];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: as for `registry_dword`; `buffer` holds `size` bytes, and RegGetValueW writes a
    // NUL-terminated REG_SZ no longer than that or fails with an error code.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let written = (size as usize / 2).min(buffer.len());
    let text = &buffer[..written];
    let end = text.iter().position(|&unit| unit == 0).unwrap_or(written);
    String::from_utf16(&text[..end]).ok()
}
