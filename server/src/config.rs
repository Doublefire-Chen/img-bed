//! Server configuration, read from a TOML file.
//!
//! ```toml
//! listen = "127.0.0.1:8080"
//! data_dir = "data"
//! frontend_dir = "../web/dist"
//! upload_limit_mb = 20
//! public_url = "https://img.aka.cy"
//! ```
//!
//! Every key is optional. Relative paths are resolved against the directory
//! containing the config file, so the server behaves the same whatever its
//! working directory is (e.g. under systemd).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
pub const DEFAULT_UPLOAD_LIMIT_MB: u64 = 20;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    /// Holds all state: the SQLite database now, Image files later.
    pub data_dir: PathBuf,
    /// The built admin frontend (`web/dist`), served under `/admin/`.
    pub frontend_dir: PathBuf,
    /// Largest accepted file, in bytes.
    pub upload_limit_bytes: u64,
    /// Scheme and host Visitors use to reach the Images, without a trailing
    /// `/` (e.g. `https://img.aka.cy`). Full URLs are this plus `/` plus the
    /// Path. Defaults to `http://<listen>`.
    pub public_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    listen: Option<SocketAddr>,
    data_dir: Option<PathBuf>,
    frontend_dir: Option<PathBuf>,
    upload_limit_mb: Option<u64>,
    public_url: Option<String>,
}

impl Config {
    /// A config with defaults, all paths relative to `base`.
    pub fn with_base(base: &Path) -> Self {
        let listen: SocketAddr = DEFAULT_LISTEN.parse().expect("valid default address");
        Self {
            listen,
            data_dir: base.join("data"),
            frontend_dir: base.join("web/dist"),
            upload_limit_bytes: DEFAULT_UPLOAD_LIMIT_MB * 1024 * 1024,
            public_url: format!("http://{listen}"),
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        let base = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Self::parse(&text, base).with_context(|| format!("parsing config file {}", path.display()))
    }

    pub fn parse(text: &str, base: &Path) -> anyhow::Result<Self> {
        let raw: RawConfig = toml::from_str(text)?;
        let defaults = Self::with_base(base);
        let limit_mb = raw.upload_limit_mb.unwrap_or(DEFAULT_UPLOAD_LIMIT_MB);
        anyhow::ensure!(limit_mb > 0, "upload_limit_mb must be at least 1");
        let listen = raw.listen.unwrap_or(defaults.listen);
        let public_url = match raw.public_url {
            Some(url) => {
                let url = url.trim_end_matches('/').to_owned();
                let host = url
                    .strip_prefix("https://")
                    .or_else(|| url.strip_prefix("http://"));
                anyhow::ensure!(
                    host.is_some_and(|h| !h.is_empty() && !h.contains(['/', '?', '#'])),
                    "public_url must look like https://img.example.com (scheme and host only)"
                );
                url
            }
            None => format!("http://{listen}"),
        };
        Ok(Self {
            listen,
            data_dir: raw
                .data_dir
                .map(|p| base.join(p))
                .unwrap_or(defaults.data_dir),
            frontend_dir: raw
                .frontend_dir
                .map(|p| base.join(p))
                .unwrap_or(defaults.frontend_dir),
            upload_limit_bytes: limit_mb * 1024 * 1024,
            public_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let c = Config::parse("", Path::new("/srv/img-bed")).unwrap();
        assert_eq!(c.listen.to_string(), DEFAULT_LISTEN);
        assert_eq!(c.data_dir, Path::new("/srv/img-bed/data"));
        assert_eq!(c.frontend_dir, Path::new("/srv/img-bed/web/dist"));
        assert_eq!(c.upload_limit_bytes, 20 * 1024 * 1024);
        assert_eq!(c.public_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn public_url_is_scheme_and_host() {
        let c = Config::parse("public_url = \"https://img.aka.cy/\"", Path::new("/")).unwrap();
        assert_eq!(c.public_url, "https://img.aka.cy");
        let c = Config::parse("listen = \"0.0.0.0:9000\"", Path::new("/")).unwrap();
        assert_eq!(c.public_url, "http://0.0.0.0:9000");
        for bad in ["img.aka.cy", "ftp://x", "https://", "https://x/sub"] {
            let text = format!("public_url = \"{bad}\"");
            assert!(Config::parse(&text, Path::new("/")).is_err(), "{bad}");
        }
    }

    #[test]
    fn relative_paths_resolve_against_config_dir_and_absolute_ones_are_kept() {
        let c = Config::parse(
            "listen = \"0.0.0.0:9000\"\ndata_dir = \"/var/lib/img-bed\"\nfrontend_dir = \"web\"\nupload_limit_mb = 5\n",
            Path::new("/etc/img-bed"),
        )
        .unwrap();
        assert_eq!(c.listen.to_string(), "0.0.0.0:9000");
        assert_eq!(c.data_dir, Path::new("/var/lib/img-bed"));
        assert_eq!(c.frontend_dir, Path::new("/etc/img-bed/web"));
        assert_eq!(c.upload_limit_bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn unknown_keys_and_zero_limit_are_rejected() {
        assert!(Config::parse("lsiten = \"x\"", Path::new("/")).is_err());
        assert!(Config::parse("upload_limit_mb = 0", Path::new("/")).is_err());
    }
}
