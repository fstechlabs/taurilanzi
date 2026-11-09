use std::path::Path;
use std::process::Command;

/// Writes an executable script from a child process. A file this process held open for writing
/// can be inherited by a process another test forks at that moment, and Linux then refuses to
/// run it ("Text file busy").
pub fn write_script(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("/bin/sh")
        .args([
            "-c",
            r#"printf '%s' "$1" > "$2" && chmod 755 "$2""#,
            "sh",
            body,
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}
