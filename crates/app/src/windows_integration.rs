use std::os::windows::ffi::OsStringExt;
use std::ptr::{null, null_mut};
use windows_sys::Win32::UI::Shell::{
    AssocQueryStringW, SHChangeNotify, ShellExecuteW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE,
    SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDOK, MB_ICONERROR, MB_OKCANCEL, SW_SHOWNORMAL,
};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const APP_NAME: &str = "simple.pdf";
const PROG_ID: &str = "SimplePDF.Document";
const CAPABILITIES_PATH: &str = "Software\\simple.pdf\\Capabilities";
const DEFAULT_APPS_URI: &str = "ms-settings:defaultapps";
const WEBVIEW2_DOWNLOAD_URL: &str =
    "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";

pub fn show_webview2_missing() {
    let message = wide_null(
        "WebView2 Runtime is required to run simple.pdf.\n\nSelect OK to open Microsoft's WebView2 download page.",
    );
    let title = wide_null("simple.pdf — missing component");

    // SAFETY: the UTF-16 buffers are NUL-terminated and live for the duration
    // of the synchronous MessageBoxW call. A null owner creates an app-modal
    // top-level message box, which is required before Tauri has initialized.
    let result = unsafe {
        MessageBoxW(
            null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_OKCANCEL,
        )
    };
    if result == IDOK {
        let _ = open_uri(WEBVIEW2_DOWNLOAD_URL);
    }
}

pub fn is_default_pdf_handler() -> Result<bool, String> {
    let association = wide_null(".pdf");
    let mut buffer = vec![0_u16; 32_768];
    let mut buffer_len = buffer.len() as u32;

    // SAFETY: the association string and output buffer are valid for this
    // synchronous call, and buffer_len reports the capacity in WCHARs.
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            association.as_ptr(),
            null(),
            buffer.as_mut_ptr(),
            &mut buffer_len,
        )
    };
    if result < 0 {
        return Ok(false);
    }

    let string_len = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer_len as usize)
        .min(buffer.len());
    let associated_exe =
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..string_len]));
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    Ok(path_key(&associated_exe) == path_key(&current_exe))
}

pub fn configure_pdf_handler() -> Result<(), String> {
    register_pdf_handler()?;
    open_uri(DEFAULT_APPS_URI)
}

fn register_pdf_handler() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let exe_display = exe.display();
    let open_command = format!("\"{exe_display}\" \"%1\"");
    let default_icon = format!("\"{exe_display}\",0");
    let application_key = format!(
        "Software\\Classes\\Applications\\{}",
        exe.file_name()
            .ok_or("The application executable has no file name")?
            .to_string_lossy()
    );
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Register a ProgID and application capabilities without taking ownership
    // of .pdf. Windows remains the only component that can change UserChoice.
    let (prog_id, _) = hkcu
        .create_subkey(format!("Software\\Classes\\{PROG_ID}"))
        .map_err(|error| error.to_string())?;
    prog_id
        .set_value("", &"PDF Document")
        .map_err(|error| error.to_string())?;

    let (icon, _) = prog_id
        .create_subkey("DefaultIcon")
        .map_err(|error| error.to_string())?;
    icon.set_value("", &default_icon)
        .map_err(|error| error.to_string())?;

    let (command, _) = prog_id
        .create_subkey("shell\\open\\command")
        .map_err(|error| error.to_string())?;
    command
        .set_value("", &open_command)
        .map_err(|error| error.to_string())?;

    let (application, _) = hkcu
        .create_subkey(application_key)
        .map_err(|error| error.to_string())?;
    let (application_command, _) = application
        .create_subkey("shell\\open\\command")
        .map_err(|error| error.to_string())?;
    application_command
        .set_value("", &open_command)
        .map_err(|error| error.to_string())?;
    let (supported_types, _) = application
        .create_subkey("SupportedTypes")
        .map_err(|error| error.to_string())?;
    supported_types
        .set_value(".pdf", &"")
        .map_err(|error| error.to_string())?;

    let (capabilities, _) = hkcu
        .create_subkey(CAPABILITIES_PATH)
        .map_err(|error| error.to_string())?;
    capabilities
        .set_value("ApplicationName", &APP_NAME)
        .map_err(|error| error.to_string())?;
    capabilities
        .set_value(
            "ApplicationDescription",
            &"Fast, small-footprint PDF reader",
        )
        .map_err(|error| error.to_string())?;
    let (file_associations, _) = capabilities
        .create_subkey("FileAssociations")
        .map_err(|error| error.to_string())?;
    file_associations
        .set_value(".pdf", &PROG_ID)
        .map_err(|error| error.to_string())?;

    let (registered_apps, _) = hkcu
        .create_subkey("Software\\RegisteredApplications")
        .map_err(|error| error.to_string())?;
    registered_apps
        .set_value(APP_NAME, &CAPABILITIES_PATH)
        .map_err(|error| error.to_string())?;

    // SAFETY: SHChangeNotify accepts null item pointers for the global
    // SHCNE_ASSOCCHANGED notification.
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, null(), null());
    }
    Ok(())
}

pub(crate) fn open_uri(uri: &str) -> Result<(), String> {
    let operation = wide_null("open");
    let target = wide_null(uri);

    // SAFETY: all string pointers are valid NUL-terminated UTF-16 buffers;
    // null optional parameters are accepted by ShellExecuteW.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(format!(
            "Windows could not open {uri} (ShellExecuteW error {})",
            result as isize
        ))
    } else {
        Ok(())
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn path_key(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{path_key, wide_null};

    #[test]
    fn wide_strings_are_nul_terminated() {
        let encoded = wide_null("simple.pdf");
        assert_eq!(encoded.last(), Some(&0));
        assert_eq!(encoded.iter().filter(|&&unit| unit == 0).count(), 1);
    }

    #[test]
    fn path_keys_ignore_windows_case_and_separator_differences() {
        assert_eq!(
            path_key(std::path::Path::new("C:/Apps/simple-pdf.exe")),
            path_key(std::path::Path::new("c:\\apps\\SIMPLE-PDF.EXE")),
        );
    }
}
