//! Expose only packaged UI assets through WebKit's private kira-ui scheme.
//! ES modules do not reliably run when index.html is opened via file://.

use std::io;
use std::path::{Component, Path, PathBuf};

const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024;
pub const UI_DOCUMENT_URI: &str = "kira-ui://app/index.html";

fn invalid_asset() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "Invalid Kira UI asset path")
}

fn asset_path(uri: &str, root: &Path) -> io::Result<(PathBuf, &'static str)> {
    let relative = uri.strip_prefix("kira-ui://app/").ok_or_else(invalid_asset)?;
    // Do not accept percent-encoded path components, queries or fragments.
    // The bundled Vite output uses simple ASCII filenames under assets/.
    if relative.is_empty()
        || relative.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        })
    {
        return Err(invalid_asset());
    }

    let relative_path = Path::new(relative);
    if !relative_path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(invalid_asset());
    }

    let mime = match relative_path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("json") => "application/json",
        _ => return Err(invalid_asset()),
    };
    let root = root.canonicalize()?;
    let target = root.join(relative_path).canonicalize()?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err(invalid_asset());
    }
    Ok((target, mime))
}

pub fn read_asset(uri: &str, root: &Path) -> io::Result<(Vec<u8>, &'static str)> {
    let (path, mime) = asset_path(uri, root)?;
    if path.metadata()?.len() > MAX_ASSET_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "UI asset too large"));
    }
    Ok((std::fs::read(path)?, mime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("kira-ui-{}-{unique}", std::process::id()));
            fs::create_dir_all(root.join("assets")).unwrap();
            fs::write(root.join("index.html"), "<html></html>").unwrap();
            fs::write(root.join("assets/app.js"), "console.log('Kira')").unwrap();
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn permits_packaged_document_and_javascript() {
        let fixture = Fixture::new();
        let (html, mime) = read_asset(UI_DOCUMENT_URI, &fixture.root).unwrap();
        assert_eq!(mime, "text/html; charset=utf-8");
        assert_eq!(html, b"<html></html>");
        let (js, mime) = read_asset("kira-ui://app/assets/app.js", &fixture.root).unwrap();
        assert_eq!(mime, "text/javascript; charset=utf-8");
        assert!(js.starts_with(b"console"));
    }

    #[test]
    fn rejects_external_origins_and_traversal() {
        let fixture = Fixture::new();
        for uri in [
            "file:///etc/passwd",
            "https://example.com/",
            "kira-ui://evil/index.html",
            "kira-ui://app/../secret.js",
            "kira-ui://app/%2e%2e/secret.js",
            "kira-ui://app/assets/../../secret.js",
            "kira-ui://app/assets/app.js?x=1",
            "kira-ui://app/assets//app.js",
        ] {
            assert!(read_asset(uri, &fixture.root).is_err(), "{uri}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_ui_directory() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        symlink("/etc/passwd", fixture.root.join("assets/escape.js")).unwrap();
        assert!(read_asset("kira-ui://app/assets/escape.js", &fixture.root).is_err());
    }

    #[test]
    fn missing_resource_fails_with_error() {
        let fixture = Fixture::new();
        assert!(read_asset("kira-ui://app/assets/unknown.js", &fixture.root).is_err());
    }
}
