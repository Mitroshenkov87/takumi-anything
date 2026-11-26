//! REA (`rea-agents`, github.com/morluto/rea): a reverse-engineering toolkit that runs as an MCP
//! server. Settings → Plugins installs it with one button: a pinned Node.js runtime and the pinned
//! REA package go into the engine store, each checked against its SHA-256, and the plugin is
//! pointed at the managed Ghidra and its Java.

use crate::table::{ArchiveFormat, Pin};
use craft_anything_evidence::{ErrorCode, ToolError};
use craft_anything_platform::process::Cancel;
use std::path::{Path, PathBuf};

/// The REA release Takumi Anything installs.
pub const VERSION: &str = "6.10.1";
/// The Node.js release REA runs on (REA asks for ^22.23.4, ^24.22.1 or >=26.12.1).
pub const NODE_VERSION: &str = "26.13.1";
/// Its engine-store folders.
pub const FOLDER: &str = "rea";
pub const NODE_FOLDER: &str = "node";
/// The plugin id and the name people see.
pub const PLUGIN_ID: &str = "rea";
pub const PLUGIN_NAME: &str = "REA";

/// rea-agents 6.10.1 from the npm registry: one package for every OS, with its dependencies and
/// native parts bundled. SHA-256 computed from the file, whose SHA-512 matches the registry's
/// published `integrity` (sha512-3AWrfFEX…).
pub const PIN: Pin = Pin {
    os: None,
    arch: None,
    url: "https://registry.npmjs.org/rea-agents/-/rea-agents-6.10.1.tgz",
    sha256: "e8cf08c9fa954e998fc8868c23fa8e5f518cdb9553488ec227b31a3bb6903150",
    format: ArchiveFormat::TarGz,
    strip_prefix: Some("package/"),
};

/// Node.js 26.13.1, SHA-256 as published in nodejs.org/dist/v26.13.1/SHASUMS256.txt.
pub const NODE_PINS: &[Pin] = &[
    Pin {
        os: Some("windows"),
        arch: Some("x86_64"),
        url: "https://nodejs.org/dist/v26.13.1/node-v26.13.1-win-x64.zip",
        sha256: "e2edda75bfb089654705cf65a13b58933bfd5f57148e8ebda916f58a2474f1f7",
        format: ArchiveFormat::Zip,
        strip_prefix: Some("node-v26.13.1-win-x64/"),
    },
    Pin {
        os: Some("linux"),
        arch: Some("x86_64"),
        url: "https://nodejs.org/dist/v26.13.1/node-v26.13.1-linux-x64.tar.xz",
        sha256: "5c063e46bab0fbc1c17cc445d7ff129cbdcd0471be6ed83b6ab864f8143bc864",
        format: ArchiveFormat::TarXz,
        strip_prefix: Some("node-v26.13.1-linux-x64/"),
    },
    Pin {
        os: Some("linux"),
        arch: Some("aarch64"),
        url: "https://nodejs.org/dist/v26.13.1/node-v26.13.1-linux-arm64.tar.xz",
        sha256: "89e1853138db51e8b82d3e98ba9c0d3f2956a28e5e55ea1b7e12d25aae92d5dd",
        format: ArchiveFormat::TarXz,
        strip_prefix: Some("node-v26.13.1-linux-arm64/"),
    },
    Pin {
        os: Some("macos"),
        arch: Some("aarch64"),
        url: "https://nodejs.org/dist/v26.13.1/node-v26.13.1-darwin-arm64.tar.gz",
        sha256: "aec2514ea1ceb2ba52455e9ec65986f6749a06dd759e5db2a72e6889668c9735",
        format: ArchiveFormat::TarGz,
        strip_prefix: Some("node-v26.13.1-darwin-arm64/"),
    },
    Pin {
        os: Some("macos"),
        arch: Some("x86_64"),
        url: "https://nodejs.org/dist/v26.13.1/node-v26.13.1-darwin-x64.tar.gz",
        sha256: "82bb96078a24ca50b9c9a38f2379e6be6d87171fcd400fc14c8934b3bce96e2f",
        format: ArchiveFormat::TarGz,
        strip_prefix: Some("node-v26.13.1-darwin-x64/"),
    },
];

/// The Node.js pin for this computer, if there is one.
pub fn node_pin() -> Option<&'static Pin> {
    NODE_PINS.iter().find(|p| p.matches(std::env::consts::OS, std::env::consts::ARCH))
}

/// The managed Node.js program in the engine store (it may not exist).
pub fn node_exe(store: &Path) -> PathBuf {
    let dir = store.join(NODE_FOLDER);
    if cfg!(windows) { dir.join("node.exe") } else { dir.join("bin").join("node") }
}

/// REA's start script in the engine store (it may not exist).
pub fn script(store: &Path) -> PathBuf {
    store.join(FOLDER).join("scripts").join("rea.mjs")
}

/// The REA version unpacked in the engine store, from its package.json.
pub fn installed_version(store: &Path) -> Option<String> {
    let text = std::fs::read_to_string(store.join(FOLDER).join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("version").and_then(|v| v.as_str()).map(str::to_string)
}

/// Whether the pinned REA and the Node.js it runs on are both in the engine store.
pub fn installed(store: &Path) -> bool {
    installed_version(store).as_deref() == Some(VERSION) && script(store).is_file() && node_exe(store).is_file()
}

/// Where REA was put.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub node: PathBuf,
    pub script: PathBuf,
}

/// Download, check and unpack Node.js (unless the managed one is there) and REA (unless the
/// pinned version is there) into the engine store. `progress` gets (what, bytes so far, total).
pub fn install(store: &Path, cancel: Option<Cancel>, progress: &dyn Fn(&str, u64, Option<u64>)) -> Result<Installed, ToolError> {
    let node = node_pin().ok_or_else(|| {
        ToolError::new(ErrorCode::NotSupported, format!("REA can't be installed for you on {} ({}).", std::env::consts::OS, std::env::consts::ARCH))
            .with_remediation("Install Node.js and rea-agents by hand, then add REA as a plugin with its command.")
    })?;
    if !node_exe(store).is_file() {
        crate::ghidra::get::fetch(node, store, NODE_FOLDER, "Node.js", cancel.clone(), progress)?;
    }
    if installed_version(store).as_deref() != Some(VERSION) || !script(store).is_file() {
        crate::ghidra::get::fetch(&PIN, store, FOLDER, "REA", cancel, progress)?;
    }
    let (node, script) = (node_exe(store), script(store));
    if !node.is_file() || !script.is_file() {
        return Err(ToolError::new(ErrorCode::GarbledOutput, "REA was unpacked, but its files weren't where we expected."));
    }
    Ok(Installed { node, script })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pin_is_versioned_and_strips_its_own_folder() {
        for p in NODE_PINS.iter().chain([&PIN]) {
            assert_eq!(p.sha256.len(), 64, "{}", p.url);
            assert!(p.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{}", p.url);
            assert!(p.url.starts_with("https://"), "{}", p.url);
        }
        for p in NODE_PINS {
            assert!(p.url.contains(&format!("/v{NODE_VERSION}/")), "{}", p.url);
            let file = p.url.rsplit('/').next().unwrap_or_default();
            let top = file.trim_end_matches(".zip").trim_end_matches(".tar.xz").trim_end_matches(".tar.gz");
            assert_eq!(p.strip_prefix, Some(format!("{top}/").as_str()));
        }
        assert!(PIN.url.ends_with(&format!("rea-agents-{VERSION}.tgz")));
    }

    #[test]
    fn windows_x64_and_the_usual_hosts_have_a_node() {
        for (os, arch) in [("windows", "x86_64"), ("linux", "x86_64"), ("linux", "aarch64"), ("macos", "aarch64"), ("macos", "x86_64")] {
            assert_eq!(NODE_PINS.iter().filter(|p| p.matches(os, arch)).count(), 1, "{os} {arch}");
        }
    }

    #[test]
    fn only_the_pinned_version_counts_as_installed() {
        let d = tempfile::tempdir().unwrap();
        assert!(!installed(d.path()));
        std::fs::create_dir_all(script(d.path()).parent().unwrap()).unwrap();
        std::fs::write(script(d.path()), "").unwrap();
        std::fs::create_dir_all(node_exe(d.path()).parent().unwrap()).unwrap();
        std::fs::write(node_exe(d.path()), "").unwrap();
        std::fs::write(d.path().join(FOLDER).join("package.json"), r#"{"name": "rea-agents", "version": "6.9.0"}"#).unwrap();
        assert!(!installed(d.path()));
        std::fs::write(d.path().join(FOLDER).join("package.json"), format!(r#"{{"name": "rea-agents", "version": "{VERSION}"}}"#)).unwrap();
        assert!(installed(d.path()));
    }
}
