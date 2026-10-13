//! Shows a file in the platform file manager with the file selected. Paths are passed
//! as program arguments, never through a shell, and the file manager is started off
//! the UI thread so a slow start never stalls rendering.
use skyline_core::view::display_path;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Starts the file manager for `path` (a file). Fails only when nothing can be started.
pub(crate) fn reveal(path: PathBuf) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} no longer exists; rescan to refresh",
                display_path(&path)
            ),
        ));
    }
    std::thread::Builder::new()
        .name("skyline-reveal".into())
        .spawn(move || {
            // Each attempt waits for its launcher so a failure can fall through to the
            // next; the file manager itself keeps running independently.
            for mut command in commands(&path) {
                let status = command
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if accepted(&status) {
                    return;
                }
            }
        })?;
    Ok(())
}

/// Windows Explorer reports exit code 1 even when it opened the window.
#[cfg(windows)]
fn accepted(status: &io::Result<std::process::ExitStatus>) -> bool {
    status.is_ok()
}

#[cfg(not(windows))]
fn accepted(status: &io::Result<std::process::ExitStatus>) -> bool {
    status.as_ref().is_ok_and(|status| status.success())
}

/// Launchers to try in order.
#[cfg(windows)]
pub(crate) fn commands(path: &Path) -> Vec<Command> {
    use std::os::windows::process::CommandExt;
    // Explorer does not accept `\\?\` verbatim paths, and needs `/select,` and the
    // quoted path in one argument; Windows paths cannot contain `"`.
    let mut explorer = Command::new("explorer.exe");
    explorer.raw_arg(format!("/select,\"{}\"", display_path(path)));
    vec![explorer]
}

#[cfg(target_os = "macos")]
pub(crate) fn commands(path: &Path) -> Vec<Command> {
    let mut open = Command::new("open");
    open.arg("-R").arg(path);
    vec![open]
}

/// The freedesktop FileManager1 interface (Nautilus, Dolphin, Nemo, Caja, Thunar…)
/// selects the file; otherwise the containing folder is opened.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn commands(path: &Path) -> Vec<Command> {
    let mut show = Command::new("dbus-send");
    show.args([
        "--session",
        "--print-reply",
        "--dest=org.freedesktop.FileManager1",
        "--type=method_call",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1.ShowItems",
    ])
    .arg(format!("array:string:{}", file_uri(path)))
    .arg("string:");
    let mut open = Command::new("xdg-open");
    open.arg(path.parent().unwrap_or(path));
    vec![show, open]
}

/// A `file://` URI with every byte outside the unreserved set percent-encoded. Also
/// keeps `dbus-send` from reading commas in a name as array separators.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn file_uris_encode_spaces_punctuation_and_non_ascii_bytes() {
        assert_eq!(
            file_uri(Path::new("/roms/Mario & Luigi, Vol. 1 (USA).gba")),
            "file:///roms/Mario%20%26%20Luigi%2C%20Vol.%201%20%28USA%29.gba"
        );
        assert_eq!(
            file_uri(Path::new("/roms/ポケモン.gb")),
            "file:///roms/%E3%83%9D%E3%82%B1%E3%83%A2%E3%83%B3.gb"
        );
    }

    #[test]
    fn linux_selects_the_file_then_falls_back_to_its_folder() {
        let commands = commands(Path::new("/roms/A B.zip"));
        assert_eq!(commands.len(), 2);
        let show: Vec<_> = commands[0].get_args().collect();
        assert_eq!(commands[0].get_program(), "dbus-send");
        assert!(show.contains(&std::ffi::OsStr::new("array:string:file:///roms/A%20B.zip")));
        assert_eq!(commands[1].get_program(), "xdg-open");
        assert_eq!(commands[1].get_args().collect::<Vec<_>>(), ["/roms"]);
    }

    #[test]
    fn missing_files_are_reported_instead_of_opening_anything() {
        let error = reveal(PathBuf::from("/definitely/not/here.gb")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
