//! The pinned official protoc (versions.toml `[protoc]`), obtained fail-closed:
//!
//! 1. The release archive for this host is looked up in a cache directory outside the repo
//!    (`DROIDUSE_CACHE_DIR`, else the OS cache directory + `droid-use`). A cached archive
//!    whose sha256 is not the pinned one is refused, not replaced.
//! 2. Otherwise it is downloaded over HTTPS only and its sha256 is checked *before* it is
//!    written to the cache; a mismatch is refused and nothing is kept.
//! 3. On every run the verified bytes (hashed and unpacked from the same in-memory buffer)
//!    yield `bin/protoc` in a fresh temporary directory, and `protoc --version` must print
//!    exactly `libprotoc <version>`.
//!
//! `PATH` and the `PROTOC` environment variable are never consulted, and no binary is ever
//! written into the repo.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::output::hex;
use crate::{Error, ErrorKind};

/// Platform names of the official release archives that a pin may list.
pub const PLATFORMS: [&str; 5] = [
    "linux-aarch_64",
    "linux-x86_64",
    "osx-aarch_64",
    "osx-x86_64",
    "win64",
];

/// Environment variable that overrides the cache directory (absolute path, outside the repo).
pub const CACHE_ENV: &str = "DROIDUSE_CACHE_DIR";

/// Upper bound for a downloaded archive (the 36.x archives are about 4 MB).
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Pin {
    pub version: String,
    /// URL template with `{version}` and `{platform}`.
    pub url: String,
    /// Platform → lower-case hex sha256 of the release archive.
    pub sha256: BTreeMap<String, String>,
}

impl Pin {
    pub fn url_for(&self, platform: &str) -> String {
        self.url
            .replace("{version}", &self.version)
            .replace("{platform}", platform)
    }

    pub fn archive_name(&self, platform: &str) -> String {
        format!("protoc-{}-{platform}.zip", self.version)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPin {
    version: String,
    url: String,
    sha256: BTreeMap<String, String>,
}

fn pin_err(msg: impl Into<String>) -> Error {
    Error::new(
        ErrorKind::ProtocPin,
        format!("versions.toml [protoc]: {}", msg.into()),
    )
}

/// Parses the `[protoc]` table of a versions.toml text.
pub fn parse_pin(versions_toml: &str) -> Result<Pin, Error> {
    let table: toml::Table = toml::from_str(versions_toml).map_err(|e| pin_err(e.to_string()))?;
    let raw = table.get("protoc").ok_or_else(|| pin_err("missing"))?;
    let raw = RawPin::deserialize(raw.clone()).map_err(|e| pin_err(e.to_string()))?;
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = raw.version.split('.').collect();
    if !(2..=3).contains(&parts.len()) || !parts.iter().all(|p| numeric(p)) {
        return Err(pin_err(format!(
            "version {:?} must be a release number such as \"36.2\"",
            raw.version
        )));
    }
    if !raw.url.starts_with("https://")
        || !raw.url.contains("{version}")
        || !raw.url.contains("{platform}")
    {
        return Err(pin_err(
            "url must be an https:// template containing {version} and {platform}",
        ));
    }
    if raw.sha256.is_empty() {
        return Err(pin_err("[protoc.sha256] lists no platform"));
    }
    for (platform, h) in &raw.sha256 {
        if !PLATFORMS.contains(&platform.as_str()) {
            return Err(pin_err(format!(
                "unknown platform {platform:?} (known: {})",
                PLATFORMS.join(", ")
            )));
        }
        if h.len() != 64 || !h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(pin_err(format!(
                "sha256 of {platform} must be 64 lower-case hex digits"
            )));
        }
    }
    Ok(Pin {
        version: raw.version,
        url: raw.url,
        sha256: raw.sha256,
    })
}

/// The release platform for a Rust `std::env::consts::{OS, ARCH}` pair. `None` where there
/// is no official native build.
pub fn platform_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch_64"),
        ("macos", "aarch64") => Some("osx-aarch_64"),
        ("macos", "x86_64") => Some("osx-x86_64"),
        ("windows", "x86_64") => Some("win64"),
        _ => None,
    }
}

pub fn host_platform() -> Result<&'static str, Error> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    platform_for(os, arch).ok_or_else(|| {
        Error::new(
            ErrorKind::ProtocPin,
            format!("no official protoc build for this host ({os}-{arch})"),
        )
    })
}

/// A verified protoc, unpacked into a temporary directory that lives as long as this value.
#[derive(Debug)]
pub struct Protoc {
    _dir: tempfile::TempDir,
    path: PathBuf,
    version: String,
}

impl Protoc {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The cache directory: `DROIDUSE_CACHE_DIR`, else `<OS cache dir>/droid-use`.
pub fn cache_root() -> Result<PathBuf, Error> {
    if let Some(v) = std::env::var_os(CACHE_ENV) {
        return Ok(PathBuf::from(v));
    }
    dirs::cache_dir()
        .map(|d| d.join("droid-use"))
        .ok_or_else(|| {
            Error::new(
                ErrorKind::Config,
                format!("no OS cache directory on this host; set {CACHE_ENV}"),
            )
        })
}

/// `p` with symlinks resolved as far as it exists, and the rest appended lexically.
fn resolve(p: &Path) -> Result<PathBuf, Error> {
    let mut existing = p.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut out = fs::canonicalize(&existing).map_err(|e| {
        Error::new(
            ErrorKind::Config,
            format!("cannot resolve {}: {e}", existing.display()),
        )
    })?;
    for c in rest.iter().rev() {
        out.push(c);
    }
    Ok(out)
}

fn check_cache_dir(repo_root: &Path, cache: &Path) -> Result<(), Error> {
    let lexical_ok = cache.is_absolute()
        && cache
            .components()
            .all(|c| !matches!(c, Component::ParentDir | Component::CurDir));
    if !lexical_ok {
        return Err(Error::new(
            ErrorKind::Config,
            format!(
                "protoc cache directory {} must be an absolute path without `.`/`..` ({CACHE_ENV})",
                cache.display()
            ),
        ));
    }
    let root = resolve(repo_root)?;
    if resolve(cache)?.starts_with(&root) {
        return Err(Error::new(
            ErrorKind::Config,
            format!(
                "protoc cache directory {} is inside the repo; binaries are never kept in the repo ({CACHE_ENV})",
                cache.display()
            ),
        ));
    }
    Ok(())
}

fn unverified(what: &str, got: &str, want: &str) -> Error {
    Error::new(
        ErrorKind::ProtocUnverified,
        format!(
            "{what} does not match the pin in versions.toml [protoc] (sha256 {got}, pinned {want}); refusing to use it"
        ),
    )
}

/// Obtains the pinned protoc for `platform` (see the module docs). `fetch` downloads a URL.
pub fn obtain(
    pin: &Pin,
    platform: &str,
    repo_root: &Path,
    cache: &Path,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Protoc, Error> {
    let want = pin
        .sha256
        .get(platform)
        .ok_or_else(|| pin_err(format!("no sha256 for this host's platform {platform}")))?;
    check_cache_dir(repo_root, cache)?;
    let dir = cache.join("protoc").join(&pin.version);
    let zip_path = dir.join(pin.archive_name(platform));
    let bytes = match fs::read(&zip_path) {
        Ok(bytes) => {
            let got = hex(&Sha256::digest(&bytes));
            if &got != want {
                return Err(unverified(
                    &format!(
                        "cached archive {} (delete it to download again)",
                        zip_path.display()
                    ),
                    &got,
                    want,
                ));
            }
            bytes
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let url = pin.url_for(platform);
            let bytes = fetch(&url).map_err(|e| {
                Error::new(ErrorKind::Protoc, format!("cannot download {url}: {e}"))
            })?;
            let got = hex(&Sha256::digest(&bytes));
            if &got != want {
                return Err(unverified(&format!("download {url}"), &got, want));
            }
            store(&dir, &zip_path, &bytes)?;
            bytes
        }
        Err(e) => {
            return Err(Error::new(
                ErrorKind::Protoc,
                format!("cannot read {}: {e}", zip_path.display()),
            ));
        }
    };
    let (tmp, path) = extract(&bytes, platform)?;
    Ok(Protoc {
        _dir: tmp,
        path,
        version: pin.version.clone(),
    })
}

/// Writes a verified archive to the cache atomically (temp file + rename).
fn store(dir: &Path, zip_path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let fail = |e: std::io::Error| {
        Error::new(
            ErrorKind::Protoc,
            format!("cannot write {}: {e}", zip_path.display()),
        )
    };
    fs::create_dir_all(dir).map_err(fail)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(fail)?;
    tmp.write_all(bytes).map_err(fail)?;
    tmp.as_file().sync_all().map_err(fail)?;
    tmp.persist(zip_path).map_err(|e| fail(e.error))?;
    Ok(())
}

/// Unpacks `bin/protoc` (`bin/protoc.exe` on Windows) into a fresh temporary directory.
fn extract(bytes: &[u8], platform: &str) -> Result<(tempfile::TempDir, PathBuf), Error> {
    let exe = if platform == "win64" {
        "protoc.exe"
    } else {
        "protoc"
    };
    let entry = format!("bin/{exe}");
    let bad = |msg: String| Error::new(ErrorKind::Protoc, format!("protoc archive: {msg}"));
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(e.to_string()))?;
    let mut file = archive
        .by_name(&entry)
        .map_err(|e| bad(format!("{entry}: {e}")))?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|e| bad(format!("{entry}: {e}")))?;
    let tmp = tempfile::Builder::new()
        .prefix("droiduse-protoc-")
        .tempdir()
        .map_err(|e| bad(format!("cannot create a temporary directory: {e}")))?;
    let bin = tmp.path().join("bin");
    let path = bin.join(exe);
    let write = || -> std::io::Result<()> {
        fs::create_dir(&bin)?;
        fs::write(&path, &content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    };
    write().map_err(|e| bad(format!("cannot unpack {entry}: {e}")))?;
    Ok((tmp, path))
}

/// `protoc --version` output must be exactly `libprotoc <version>`.
pub fn check_version_output(out: &str, version: &str) -> Result<(), Error> {
    let want = format!("libprotoc {version}");
    if out.trim_end_matches(['\r', '\n']) == want {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::ProtocUnverified,
            format!(
                "protoc --version printed {:?}, expected {want:?}; refusing to use it",
                out.trim_end()
            ),
        ))
    }
}

fn run_version(p: &Protoc) -> Result<(), Error> {
    let out = Command::new(p.path())
        .arg("--version")
        .output()
        .map_err(|e| Error::new(ErrorKind::Protoc, format!("cannot run protoc: {e}")))?;
    if !out.status.success() {
        return Err(Error::new(
            ErrorKind::Protoc,
            format!("protoc --version failed: {}", out.status),
        ));
    }
    check_version_output(&String::from_utf8_lossy(&out.stdout), &p.version)
}

/// Downloads `url` over HTTPS only (redirects too), up to [`MAX_ARCHIVE`] bytes.
pub fn https_fetch(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(MAX_ARCHIVE)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// The pinned protoc for this host, ready to run.
pub fn acquire(repo_root: &Path) -> Result<Protoc, Error> {
    let versions = repo_root.join("versions.toml");
    let text = fs::read_to_string(&versions).map_err(|e| {
        Error::new(
            ErrorKind::ProtocPin,
            format!("cannot read {}: {e}", versions.display()),
        )
    })?;
    let pin = parse_pin(&text)?;
    let platform = host_platform()?;
    let p = obtain(&pin, platform, repo_root, &cache_root()?, &https_fetch)?;
    run_version(&p)?;
    Ok(p)
}
