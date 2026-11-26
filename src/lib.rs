// SPDX-License-Identifier: Apache-2.0
//!
//! SigmaOS-plugin-marketplace — plugin catalog & installer.
//!
//! A marketplace is a catalog of available plugins (metadata + download
//! info) with search, install, update, and uninstall workflows. It
//! delegates actual installation to a `MarketBackend` (which typically
//! wraps the plugin-registry + a fetch source). Keeps an install
//! history and supports pinned versions.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// A catalog entry (what's available).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    /// Latest version string.
    pub version: String,
    /// All known versions (newest first).
    pub versions: Vec<String>,
    pub description: String,
    /// kind: extension|theme|driver|service|app.
    pub kind: String,
    /// author/maintainer.
    pub author: String,
    /// download URL or local path.
    pub source: String,
    /// approximate size bytes.
    pub size_bytes: u64,
    /// user rating 0-5.
    pub rating: f32,
    /// download count.
    pub downloads: u64,
    /// tags for search.
    pub tags: Vec<String>,
    /// license SPDX id.
    pub license: String,
}

/// An installed record (what's on the system).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledEntry {
    pub id: String,
    pub version: String,
    pub installed_unix: u64,
    /// pinned = don't auto-update.
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarketError {
    NotInCatalog(String),
    AlreadyInstalled(String),
    NotInstalled(String),
    BadVersion(String),
    Backend(String),
    NoUpdates,
    InstallFailed(String),
}
impl fmt::Display for MarketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInCatalog(id) => write!(f, "not in catalog: {id}"),
            Self::AlreadyInstalled(id) => write!(f, "{id} already installed"),
            Self::NotInstalled(id) => write!(f, "{id} not installed"),
            Self::BadVersion(v) => write!(f, "bad version {v}"),
            Self::Backend(e) => write!(f, "backend: {e}"),
            Self::NoUpdates => write!(f, "no updates available"),
            Self::InstallFailed(e) => write!(f, "install: {e}"),
        }
    }
}
impl std::error::Error for MarketError {}

/// Backend for install/uninstall operations.
pub trait MarketBackend {
    /// Install plugin id@version; returns install path.
    fn install(&self, id: &str, version: &str) -> Result<PathBuf, String>;
    /// Remove installed plugin.
    fn uninstall(&self, id: &str) -> Result<(), String>;
    /// Is plugin currently installed (backend's view)?
    fn is_installed(&self, id: &str) -> bool;
    /// Current installed version if any.
    fn installed_version(&self, id: &str) -> Option<String>;
    /// Current unix time.
    fn now_unix(&self) -> u64;
}

/// Host backend: shells out to sigmaos-plugins CLI or direct registry.
pub struct HostBackend;
impl MarketBackend for HostBackend {
    fn install(&self, id: &str, version: &str) -> Result<PathBuf, String> {
        // Delegate to plugin-registry HostBackend via env-configured dir.
        // For now: run the sigmaos-plugins binary if on PATH, else error.
        let out = std::process::Command::new("sigmaos-plugins")
            .args(["install", id, version])
            .output();
        match out {
            Ok(o) if o.status.success() => Ok(PathBuf::from(format!(
                "/usr/lib/sigmaos/plugins/{}-{}",
                id, version
            ))),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Err(format!("sigmaos-plugins unavailable: {}", e)),
        }
    }
    fn uninstall(&self, id: &str) -> Result<(), String> {
        let out = std::process::Command::new("sigmaos-plugins")
            .args(["remove", id])
            .output();
        match out {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Err(format!("sigmaos-plugins unavailable: {}", e)),
        }
    }
    fn is_installed(&self, id: &str) -> bool {
        self.installed_version(id).is_some()
    }
    fn installed_version(&self, id: &str) -> Option<String> {
        // read registry state file
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let p = PathBuf::from(home).join(".local/share/sigmaos/plugins.json");
        let txt = std::fs::read_to_string(p).ok()?;
        let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
        v.get("plugins")?
            .as_array()?
            .iter()
            .find(|pl| pl.pointer("/manifest/id").and_then(|x| x.as_str()) == Some(id))
            .and_then(|pl| {
                pl.pointer("/manifest/version")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
            })
    }
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// In-memory backend for tests.
pub struct FakeBackend {
    pub installed: std::cell::RefCell<BTreeMap<String, (String, u64)>>, // id → (version, unix)
    pub now: std::cell::Cell<u64>,
    pub fail_install: std::cell::Cell<bool>,
}
impl FakeBackend {
    pub fn new() -> Self {
        Self {
            installed: std::cell::RefCell::new(BTreeMap::new()),
            now: std::cell::Cell::new(1_700_000_000),
            fail_install: std::cell::Cell::new(false),
        }
    }
}
impl Default for FakeBackend {
    fn default() -> Self {
        Self::new()
    }
}
impl MarketBackend for FakeBackend {
    fn install(&self, id: &str, version: &str) -> Result<PathBuf, String> {
        if self.fail_install.get() {
            return Err("network error".into());
        }
        self.installed
            .borrow_mut()
            .insert(id.into(), (version.into(), self.now.get()));
        Ok(PathBuf::from(format!("/fake/{}-{}", id, version)))
    }
    fn uninstall(&self, id: &str) -> Result<(), String> {
        self.installed.borrow_mut().remove(id);
        Ok(())
    }
    fn is_installed(&self, id: &str) -> bool {
        self.installed.borrow().contains_key(id)
    }
    fn installed_version(&self, id: &str) -> Option<String> {
        self.installed.borrow().get(id).map(|(v, _)| v.clone())
    }
    fn now_unix(&self) -> u64 {
        self.now.get()
    }
}

/// The marketplace.
pub struct SigmaOSPluginMarketplace {
    /// Catalog: id → entry.
    pub catalog: BTreeMap<String, CatalogEntry>,
    /// Install records.
    pub installed: BTreeMap<String, InstalledEntry>,
}

impl SigmaOSPluginMarketplace {
    pub fn new() -> Self {
        let mut m = Self {
            catalog: BTreeMap::new(),
            installed: BTreeMap::new(),
        };
        m.seed_catalog();
        m
    }

    fn seed_catalog(&mut self) {
        type Seed = (
            &'static str,
            &'static str,
            &'static str,
            &'static [&'static str],
            &'static str,
            &'static str,
            &'static [&'static str],
            f32,
            u64,
            &'static str,
        );
        let seeds: [Seed; 6] = [
            (
                "weather",
                "Weather Widget",
                "1.2.0",
                &["1.2.0", "1.1.0", "1.0.0"],
                "extension",
                "Shows current weather in panel",
                &["weather", "panel", "info"],
                4.5,
                15234,
                "Apache-2.0",
            ),
            (
                "clipboard+",
                "Clipboard History+",
                "2.0.1",
                &["2.0.1", "2.0.0", "1.5.0"],
                "extension",
                "Persistent clipboard history with search",
                &["clipboard", "history", "productivity"],
                4.8,
                45210,
                "MIT",
            ),
            (
                "nightlight",
                "Night Light",
                "1.0.3",
                &["1.0.3", "1.0.2"],
                "service",
                "Color temperature scheduler",
                &["display", "eye-care", "schedule"],
                4.2,
                8934,
                "GPL-3.0",
            ),
            (
                "podcast",
                "Podcast Client",
                "0.9.7",
                &["0.9.7", "0.9.6"],
                "app",
                "RSS podcast player with downloads",
                &["media", "rss", "podcast"],
                4.0,
                3210,
                "MIT",
            ),
            (
                "ssh-agent",
                "SSH Agent Panel",
                "1.1.0",
                &["1.1.0", "1.0.0"],
                "extension",
                "Visual SSH agent/key manager",
                &["ssh", "security", "keys"],
                4.6,
                6789,
                "Apache-2.0",
            ),
            (
                "theme-pack",
                "Community Theme Pack",
                "3.0.0",
                &["3.0.0", "2.9.0"],
                "theme",
                "20 community color themes",
                &["theme", "appearance", "colors"],
                4.4,
                22100,
                "CC0-1.0",
            ),
        ];
        for (id, name, ver, vers, kind, desc, tags, rating, downloads, lic) in seeds {
            self.catalog.insert(
                id.into(),
                CatalogEntry {
                    id: id.into(),
                    name: name.into(),
                    version: ver.into(),
                    versions: vers.iter().map(|s| s.to_string()).collect(),
                    description: desc.into(),
                    kind: kind.into(),
                    author: "SigmaOS Community".into(),
                    source: format!("sigmaos-catalog://{}", id),
                    size_bytes: 1024 * 100,
                    rating,
                    downloads,
                    tags: tags.iter().map(|s| s.to_string()).collect(),
                    license: lic.into(),
                },
            );
        }
    }

    /// Add a catalog entry (e.g. from a remote marketplace index).
    pub fn add_entry(&mut self, e: CatalogEntry) -> Result<(), MarketError> {
        if self.catalog.contains_key(&e.id) {
            // update existing
            self.catalog.insert(e.id.clone(), e);
            return Ok(());
        }
        self.catalog.insert(e.id.clone(), e);
        Ok(())
    }

    /// Search catalog by query (matches id, name, description, tags).
    /// Returns entries sorted by relevance (name>id>tag>desc) then rating.
    pub fn search(&self, query: &str) -> Vec<&CatalogEntry> {
        let q = query.to_lowercase();
        let mut scored: Vec<(i32, &CatalogEntry)> = vec![];
        for e in self.catalog.values() {
            let mut score = 0i32;
            if e.id.to_lowercase() == q {
                score += 100;
            }
            if e.name.to_lowercase().contains(&q) {
                score += 50;
            }
            if e.tags.iter().any(|t| t.to_lowercase().contains(&q)) {
                score += 30;
            }
            if e.description.to_lowercase().contains(&q) {
                score += 10;
            }
            if score > 0 {
                // rating as tiebreak
                score += (e.rating * 10.0) as i32;
                scored.push((score, e));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.downloads.cmp(&a.1.downloads)));
        scored.into_iter().map(|(_, e)| e).collect()
    }

    /// Browse by kind.
    pub fn browse(&self, kind: &str) -> Vec<&CatalogEntry> {
        self.catalog.values().filter(|e| e.kind == kind).collect()
    }

    /// Get catalog entry.
    pub fn entry(&self, id: &str) -> Option<&CatalogEntry> {
        self.catalog.get(id)
    }

    /// Install a plugin from catalog.
    pub fn install<B: MarketBackend>(
        &mut self,
        be: &B,
        id: &str,
        version: Option<&str>,
    ) -> Result<(), MarketError> {
        let entry = self
            .catalog
            .get(id)
            .ok_or_else(|| MarketError::NotInCatalog(id.into()))?;
        if self.installed.contains_key(id) {
            return Err(MarketError::AlreadyInstalled(id.into()));
        }
        let ver = version.unwrap_or(&entry.version);
        if !entry.versions.contains(&ver.to_string()) {
            return Err(MarketError::BadVersion(ver.into()));
        }
        let path = be.install(id, ver).map_err(MarketError::Backend)?;
        let _ = path;
        self.installed.insert(
            id.into(),
            InstalledEntry {
                id: id.into(),
                version: ver.into(),
                installed_unix: be.now_unix(),
                pinned: version.is_some(),
            },
        );
        Ok(())
    }

    /// Uninstall.
    pub fn uninstall<B: MarketBackend>(&mut self, be: &B, id: &str) -> Result<(), MarketError> {
        if !self.installed.contains_key(id) {
            return Err(MarketError::NotInstalled(id.into()));
        }
        be.uninstall(id).map_err(MarketError::Backend)?;
        self.installed.remove(id);
        Ok(())
    }

    /// Check for updates: catalog version != installed version.
    pub fn updates_available(&self) -> Vec<(String, String, String)> {
        // returns (id, installed_version, catalog_version)
        let mut out = vec![];
        for inst in self.installed.values() {
            if inst.pinned {
                continue;
            }
            if let Some(cat) = self.catalog.get(&inst.id) {
                if cat.version != inst.version {
                    out.push((inst.id.clone(), inst.version.clone(), cat.version.clone()));
                }
            }
        }
        out
    }

    /// Update all unpinned installs that have newer catalog versions.
    pub fn update_all<B: MarketBackend>(&mut self, be: &B) -> Result<Vec<String>, MarketError> {
        let updates = self.updates_available();
        if updates.is_empty() {
            return Err(MarketError::NoUpdates);
        }
        let mut updated = vec![];
        for (id, old_ver, new_ver) in updates {
            // uninstall old, install new
            be.uninstall(&id).map_err(MarketError::Backend)?;
            be.install(&id, &new_ver).map_err(MarketError::Backend)?;
            if let Some(i) = self.installed.get_mut(&id) {
                i.version = new_ver.clone();
            }
            updated.push(format!("{}: {} → {}", id, old_ver, new_ver));
        }
        Ok(updated)
    }

    /// Installed entries.
    pub fn installed(&self) -> impl Iterator<Item = &InstalledEntry> {
        self.installed.values()
    }

    /// Stats.
    pub fn stats(&self) -> (usize, usize) {
        (self.catalog.len(), self.installed.len())
    }

    pub fn status_line(&self) -> String {
        let (c, i) = self.stats();
        format!("marketplace: {} catalog entries, {} installed", c, i)
    }
}

impl Default for SigmaOSPluginMarketplace {
    fn default() -> Self {
        Self::new()
    }
}

// ---- persistence ----
#[derive(serde::Serialize, serde::Deserialize)]
struct Persist {
    catalog: Vec<CatalogEntry>,
    installed: Vec<InstalledEntry>,
}
impl SigmaOSPluginMarketplace {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let p = Persist {
            catalog: self.catalog.values().cloned().collect(),
            installed: self.installed.values().cloned().collect(),
        };
        let j = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, j).map_err(|e| e.to_string())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let txt = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let p: Persist = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        let mut m = Self {
            catalog: BTreeMap::new(),
            installed: BTreeMap::new(),
        };
        for e in p.catalog {
            m.catalog.insert(e.id.clone(), e);
        }
        for i in p.installed {
            m.installed.insert(i.id.clone(), i);
        }
        Ok(m)
    }
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("SIGMAOS_MARKET_FILE") {
            return PathBuf::from(p);
        }
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
            .join(".local/share/sigmaos/marketplace.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_catalog_present() {
        let m = SigmaOSPluginMarketplace::new();
        assert!(m.catalog.len() >= 6);
        assert!(m.entry("weather").is_some());
        assert!(m.entry("clipboard+").is_some());
    }

    #[test]
    fn search_relevance() {
        let m = SigmaOSPluginMarketplace::new();
        let r = m.search("weather");
        assert_eq!(r[0].id, "weather");
        let r2 = m.search("clipboard");
        assert!(r2.iter().any(|e| e.id == "clipboard+"));
        let r3 = m.search("ssh");
        assert!(r3.iter().any(|e| e.id == "ssh-agent"));
        let r4 = m.search("nonexistent-xyz");
        assert!(r4.is_empty());
    }

    #[test]
    fn browse_by_kind() {
        let m = SigmaOSPluginMarketplace::new();
        let themes = m.browse("theme");
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "theme-pack");
        let apps = m.browse("app");
        assert_eq!(apps.len(), 1);
    }

    #[test]
    fn install_and_uninstall() {
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        m.install(&be, "weather", None).unwrap();
        assert!(be.is_installed("weather"));
        assert_eq!(m.installed.get("weather").unwrap().version, "1.2.0"); // latest
                                                                          // duplicate rejected
        assert!(m.install(&be, "weather", None).is_err());
        // specific version
        let mut m2 = SigmaOSPluginMarketplace::new();
        m2.install(&be, "weather", Some("1.1.0")).unwrap();
        assert_eq!(m2.installed.get("weather").unwrap().version, "1.1.0");
        assert!(m2.installed.get("weather").unwrap().pinned);
        // bad version rejected
        assert!(m2.install(&be, "nightlight", Some("9.9.9")).is_err());
        // uninstall
        m.uninstall(&be, "weather").unwrap();
        assert!(!be.is_installed("weather"));
        assert!(m.uninstall(&be, "weather").is_err());
    }

    #[test]
    fn install_not_in_catalog() {
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        assert!(m.install(&be, "fake-plugin", None).is_err());
    }

    #[test]
    fn backend_failure_propagates() {
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        be.fail_install.set(true);
        assert!(m.install(&be, "weather", None).is_err());
        assert!(!m.installed.contains_key("weather"));
    }

    #[test]
    fn updates_detection() {
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        // install old version of weather (1.1.0) — but catalog latest is 1.2.0
        be.installed
            .borrow_mut()
            .insert("weather".into(), ("1.1.0".into(), 100u64));
        m.installed.insert(
            "weather".into(),
            InstalledEntry {
                id: "weather".into(),
                version: "1.1.0".into(),
                installed_unix: 100,
                pinned: false,
            },
        );
        let updates = m.updates_available();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, "weather");
        assert_eq!(updates[0].1, "1.1.0");
        assert_eq!(updates[0].2, "1.2.0");
        // pinned → no update
        m.installed.get_mut("weather").unwrap().pinned = true;
        assert!(m.updates_available().is_empty());
    }

    #[test]
    fn update_all() {
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        // two outdated installs
        be.installed
            .borrow_mut()
            .insert("weather".into(), ("1.1.0".into(), 100u64));
        be.installed
            .borrow_mut()
            .insert("nightlight".into(), ("1.0.2".into(), 100u64));
        m.installed.insert(
            "weather".into(),
            InstalledEntry {
                id: "weather".into(),
                version: "1.1.0".into(),
                installed_unix: 100,
                pinned: false,
            },
        );
        m.installed.insert(
            "nightlight".into(),
            InstalledEntry {
                id: "nightlight".into(),
                version: "1.0.2".into(),
                installed_unix: 100,
                pinned: false,
            },
        );
        let updated = m.update_all(&be).unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(m.installed.get("weather").unwrap().version, "1.2.0");
        assert_eq!(m.installed.get("nightlight").unwrap().version, "1.0.3");
        // second call → no updates
        assert!(m.update_all(&be).is_err());
    }

    #[test]
    fn catalog_entry_management() {
        let mut m = SigmaOSPluginMarketplace::new();
        let e = CatalogEntry {
            id: "custom".into(),
            name: "Custom".into(),
            version: "0.1.0".into(),
            versions: vec!["0.1.0".into()],
            description: "test".into(),
            kind: "extension".into(),
            author: "me".into(),
            source: "local".into(),
            size_bytes: 0,
            rating: 5.0,
            downloads: 0,
            tags: vec![],
            license: "MIT".into(),
        };
        m.add_entry(e).unwrap();
        assert!(m.entry("custom").is_some());
        // update entry
        let e2 = CatalogEntry {
            version: "0.2.0".into(),
            versions: vec!["0.2.0".into(), "0.1.0".into()],
            ..m.catalog.get("custom").unwrap().clone()
        };
        m.add_entry(e2).unwrap();
        assert_eq!(m.entry("custom").unwrap().version, "0.2.0");
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sigmaos-mkt-test-{}", std::process::id()));
        let p = dir.join("market.json");
        let mut m = SigmaOSPluginMarketplace::new();
        let be = FakeBackend::new();
        m.install(&be, "weather", None).unwrap();
        m.save(&p).unwrap();
        let m2 = SigmaOSPluginMarketplace::load(&p).unwrap();
        assert_eq!(m2.catalog.len(), 6);
        assert_eq!(m2.installed.len(), 1);
        assert!(m2.installed.contains_key("weather"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
