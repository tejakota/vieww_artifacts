//! The system's own file chooser, without a dependency.
//!
//! vieww has no native dialog, and Wright adds no crates, so this asks
//! the operating system's own tools — which also means the dialog looks
//! exactly like every other dialog on that machine:
//!
//! ```text
//!  Windows  PowerShell + System.Windows.Forms (OpenFileDialog / FolderBrowserDialog)
//!  macOS    osascript: choose file / choose folder
//!  Linux    zenity, then kdialog
//! ```
//!
//! Blocking — call it on a worker thread. Cancel returns an empty list.
//! Dropping files onto the window works everywhere and needs none of this.

use std::path::PathBuf;
use std::process::Command;

fn lines(out: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(out)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Ask for files (`folder == false`) or one folder.
pub fn pick(folder: bool) -> Result<Vec<PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = if folder {
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
             if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath }"
        } else {
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Multiselect = $true; \
             if ($d.ShowDialog() -eq 'OK') { $d.FileNames -join \"`n\" }"
        };
        let out = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("could not open the file dialog: {e}"))?;
        Ok(lines(&out.stdout))
    }
    #[cfg(target_os = "macos")]
    {
        let script: &[&str] = if folder {
            &["-e", "POSIX path of (choose folder)"]
        } else {
            &[
                "-e",
                "set fs to choose file with multiple selections allowed",
                "-e",
                "set out to \"\"",
                "-e",
                "repeat with f in fs",
                "-e",
                "set out to out & POSIX path of f & linefeed",
                "-e",
                "end repeat",
                "-e",
                "return out",
            ]
        };
        let out = Command::new("osascript")
            .args(script)
            .output()
            .map_err(|e| format!("could not open the file dialog: {e}"))?;
        // Cancel exits non-zero with "User canceled" — that is an empty answer.
        Ok(if out.status.success() {
            lines(&out.stdout)
        } else {
            Vec::new()
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let zenity: Vec<&str> = if folder {
            vec!["--file-selection", "--directory"]
        } else {
            vec!["--file-selection", "--multiple", "--separator=\n"]
        };
        if let Ok(out) = Command::new("zenity").args(&zenity).output() {
            return Ok(if out.status.success() {
                lines(&out.stdout)
            } else {
                Vec::new()
            });
        }
        let kdialog: Vec<&str> = if folder {
            vec!["--getexistingdirectory", "."]
        } else {
            vec!["--getopenfilename", ".", "--multiple", "--separate-output"]
        };
        if let Ok(out) = Command::new("kdialog").args(&kdialog).output() {
            return Ok(if out.status.success() {
                lines(&out.stdout)
            } else {
                Vec::new()
            });
        }
        Err(
            "no file dialog found (install zenity or kdialog) — or drop files onto the window"
                .into(),
        )
    }
}

/// Open a folder in the system's file manager, returning launch failures to
/// the UI rather than silently leaving the button looking broken.
pub fn reveal(dir: &std::path::Path) -> Result<(), String> {
    // File-manager launchers interpret relative paths against the app's
    // process working directory (which may be the install directory after
    // a shortcut launch). Resolve exports before handing them to the OS.
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("could not resolve folder {}: {e}", dir.display()))?;
    if !dir.is_dir() {
        return Err(format!("{} is not an existing folder", dir.display()));
    }

    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";

    Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start {program} for {}: {e}", dir.display()))
}
