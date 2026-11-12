//! Pinned protoc acquisition: the versions.toml pin, the host platform mapping, verified
//! download into the cache, refusal of anything that does not match the pin (fail closed),
//! and the version check. No network access and nothing is executed: downloads go through
//! an injected fetcher and the archives are built here.

use std::cell::Cell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use codegen::ErrorKind;
use codegen::protoc::{self, Pin};
use sha2::{Digest, Sha256};

const VERSIONS: &str = r#"
[protocol]
adp = "0.1.0"

[protoc]
version = "36.2"
url = "https://github.com/protocolbuffers/protobuf/releases/download/v{version}/protoc-{version}-{platform}.zip"

[protoc.sha256]
linux-x86_64 = "121f6c7afe1d4d0e3ea6aab9432038599250134cbf4474cb1167d2c7decd4278"
win64 = "f0c128dc0d8492eceece83bb459a4c0e316764b929ffbf1aa416357fd644edd3"
"#;

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

/// A release-shaped archive: `bin/<exe>` plus an include file.
fn archive(exe: &str, content: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file("include/google/protobuf/empty.proto", opts)
            .unwrap();
        z.write_all(b"syntax = \"proto3\";\n").unwrap();
        z.start_file(format!("bin/{exe}"), opts).unwrap();
        z.write_all(content).unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

/// A pin for `platform` whose hash matches `zip`.
fn pin_for(platform: &str, zip: &[u8]) -> Pin {
    let text = format!(
        "[protoc]\nversion = \"36.2\"\nurl = \"https://example.invalid/v{{version}}/protoc-{{version}}-{{platform}}.zip\"\n[protoc.sha256]\n{platform} = \"{}\"\n",
        hex(&Sha256::digest(zip))
    );
    protoc::parse_pin(&text).unwrap()
}

struct Cache {
    _dir: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
}

fn cache() -> Cache {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let cache = dir.path().join("cache");
    fs::create_dir_all(&root).unwrap();
    Cache {
        _dir: dir,
        root,
        cache,
    }
}

fn cached_zip(cache: &Path) -> PathBuf {
    cache.join("protoc/36.2/protoc-36.2-linux-x86_64.zip")
}

#[test]
fn pin_is_read_from_versions_toml() {
    let pin = protoc::parse_pin(VERSIONS).unwrap();
    assert_eq!(pin.version, "36.2");
    assert_eq!(
        pin.url_for("win64"),
        "https://github.com/protocolbuffers/protobuf/releases/download/v36.2/protoc-36.2-win64.zip"
    );
    assert_eq!(pin.sha256.len(), 2);
}

#[test]
fn the_real_pin_covers_every_ci_platform() {
    let text =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../versions.toml"))
            .unwrap();
    let pin = protoc::parse_pin(&text).unwrap();
    for p in ["linux-x86_64", "linux-aarch_64", "osx-aarch_64", "win64"] {
        assert!(pin.sha256.contains_key(p), "{p}");
    }
}

#[test]
fn pin_errors() {
    let bad = [
        // no [protoc]
        "[protocol]\nadp = \"0.1.0\"\n".to_string(),
        // short hash
        VERSIONS.replace("121f6c7afe1d4d0e", ""),
        // non-hex hash
        VERSIONS.replace("121f6c7afe1d4d0e", "121f6c7afe1d4d0z"),
        // upper-case hex is not the canonical form
        VERSIONS.replace("121f6c7afe1d4d0e", "121F6C7AFE1D4D0E"),
        // plain http
        VERSIONS.replace("https://github.com", "http://github.com"),
        // url without the placeholders
        VERSIONS.replace("{platform}", "linux"),
        // unknown platform name
        VERSIONS.replace("win64 =", "win-arm64 ="),
        // unknown key
        VERSIONS.replace("[protoc.sha256]", "mirror = \"x\"\n[protoc.sha256]"),
        // bad version
        VERSIONS.replace("version = \"36.2\"", "version = \"latest\""),
        // no hashes
        VERSIONS
            .split("[protoc.sha256]")
            .next()
            .unwrap()
            .to_string()
            + "[protoc.sha256]\n",
    ];
    for text in bad {
        let e = protoc::parse_pin(&text).unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProtocPin, "{text}");
    }
}

#[test]
fn host_platform_mapping() {
    assert_eq!(
        protoc::platform_for("linux", "x86_64"),
        Some("linux-x86_64")
    );
    assert_eq!(
        protoc::platform_for("linux", "aarch64"),
        Some("linux-aarch_64")
    );
    assert_eq!(
        protoc::platform_for("macos", "aarch64"),
        Some("osx-aarch_64")
    );
    assert_eq!(protoc::platform_for("macos", "x86_64"), Some("osx-x86_64"));
    assert_eq!(protoc::platform_for("windows", "x86_64"), Some("win64"));
    // no official build: refuse rather than run an emulated or foreign binary
    assert_eq!(protoc::platform_for("windows", "aarch64"), None);
    assert_eq!(protoc::platform_for("freebsd", "x86_64"), None);
    assert_eq!(protoc::platform_for("linux", "riscv64"), None);
}

#[test]
fn unpinned_platform_is_refused() {
    let c = cache();
    let zip = archive("protoc", b"bin");
    let pin = pin_for("linux-x86_64", &zip);
    let e = protoc::obtain(&pin, "osx-aarch_64", &c.root, &c.cache, &|_| -> Result<
        Vec<u8>,
        String,
    > {
        panic!("must not download")
    })
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProtocPin);
}

#[test]
fn verified_download_is_cached_and_extracted() {
    let c = cache();
    let zip = archive("protoc", b"#!protoc");
    let pin = pin_for("linux-x86_64", &zip);
    let calls = Cell::new(0);
    let fetch = |url: &str| -> Result<Vec<u8>, String> {
        calls.set(calls.get() + 1);
        assert_eq!(
            url,
            "https://example.invalid/v36.2/protoc-36.2-linux-x86_64.zip"
        );
        Ok(zip.clone())
    };
    let p = protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &fetch).unwrap();
    assert_eq!(fs::read(p.path()).unwrap(), b"#!protoc");
    assert!(p.path().ends_with("bin/protoc"));
    assert!(
        !p.path().starts_with(&c.cache),
        "extracted outside the cache"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(p.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
    assert_eq!(fs::read(cached_zip(&c.cache)).unwrap(), zip);
    // the second run uses the verified cache
    let p2 = protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &fetch).unwrap();
    assert_eq!(fs::read(p2.path()).unwrap(), b"#!protoc");
    assert_eq!(calls.get(), 1);
}

#[test]
fn windows_archive_extracts_protoc_exe() {
    let c = cache();
    let zip = archive("protoc.exe", b"MZ");
    let pin = pin_for("win64", &zip);
    let p = protoc::obtain(&pin, "win64", &c.root, &c.cache, &|_| Ok(zip.clone())).unwrap();
    assert!(p.path().ends_with("bin/protoc.exe"));
}

#[test]
fn download_that_does_not_match_the_pin_is_refused_and_not_cached() {
    let c = cache();
    let good = archive("protoc", b"good");
    let evil = archive("protoc", b"evil");
    let pin = pin_for("linux-x86_64", &good);
    let e = protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &|_| {
        Ok(evil.clone())
    })
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProtocUnverified);
    assert!(!cached_zip(&c.cache).exists());
    // no partial download is left behind either
    let dir = cached_zip(&c.cache).parent().unwrap().to_path_buf();
    if dir.exists() {
        assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
    }
}

#[test]
fn tampered_cache_is_refused_without_downloading() {
    let c = cache();
    let good = archive("protoc", b"good");
    let pin = pin_for("linux-x86_64", &good);
    protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &|_| {
        Ok(good.clone())
    })
    .unwrap();
    fs::write(cached_zip(&c.cache), archive("protoc", b"evil")).unwrap();
    let e = protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &|_| -> Result<
        Vec<u8>,
        String,
    > {
        panic!("must not download over a tampered cache")
    })
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProtocUnverified);
    assert!(
        e.to_string().contains("protoc-36.2-linux-x86_64.zip"),
        "{e}"
    );
}

#[test]
fn download_failure_cannot_run() {
    let c = cache();
    let pin = pin_for("linux-x86_64", b"x");
    let e = protoc::obtain(&pin, "linux-x86_64", &c.root, &c.cache, &|_| {
        Err("offline".into())
    })
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Protoc);
    assert!(e.to_string().contains("offline"));
}

#[test]
fn archive_without_protoc_is_refused() {
    let c = cache();
    let zip = archive("not-protoc", b"x");
    let pin = pin_for("linux-x86_64", &zip);
    let e = protoc::obtain(
        &pin,
        "linux-x86_64",
        &c.root,
        &c.cache,
        &|_| Ok(zip.clone()),
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Protoc);
}

#[test]
fn cache_inside_the_repo_is_refused() {
    let c = cache();
    let zip = archive("protoc", b"x");
    let pin = pin_for("linux-x86_64", &zip);
    let inside = c.root.join("target/protoc-cache");
    let e =
        protoc::obtain(&pin, "linux-x86_64", &c.root, &inside, &|_| Ok(zip.clone())).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Config);
    assert!(!inside.exists());
}

#[test]
fn relative_cache_dir_is_refused() {
    let c = cache();
    let zip = archive("protoc", b"x");
    let pin = pin_for("linux-x86_64", &zip);
    let e = protoc::obtain(&pin, "linux-x86_64", &c.root, Path::new("cache"), &|_| {
        Ok(zip.clone())
    })
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Config);
}

#[test]
fn version_output_must_match_the_pin_exactly() {
    assert!(protoc::check_version_output("libprotoc 36.2\n", "36.2").is_ok());
    assert!(protoc::check_version_output("libprotoc 36.2\r\n", "36.2").is_ok());
    for bad in [
        "libprotoc 36.1",
        "libprotoc 36.20",
        "libprotoc 36.2-rc1",
        "",
        "protoc 36.2",
    ] {
        let e = protoc::check_version_output(bad, "36.2").unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProtocUnverified, "{bad}");
    }
}
