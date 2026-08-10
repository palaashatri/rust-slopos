use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AppBundle {
    pub bundle_id: String,
    pub name: String,
    pub version: String,
    pub path: String,
    pub entrypoint: String,
    pub supported_types: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FileAssociation {
    pub extension: String,
    pub default_app: String,
    pub user_override: Option<String>,
}

pub struct LaunchServices {
    pub bundles: HashMap<String, AppBundle>,
    pub associations: HashMap<String, FileAssociation>,
    pub search_paths: Vec<String>,
}

impl Default for LaunchServices {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchServices {
    pub fn new() -> Self {
        let mut search_paths = vec!["/Applications".into(), "/User/Applications".into()];
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                search_paths.push(format!("{home}/Applications"));
            }
        }
        let mut services = Self {
            bundles: HashMap::new(),
            associations: HashMap::new(),
            search_paths,
        };
        services.setup_default_associations();
        services
    }

    fn setup_default_associations(&mut self) {
        let defaults = vec![
            ("txt", "com.slopos.textedit"),
            ("rtf", "com.slopos.textedit"),
            ("md", "com.slopos.textedit"),
            ("png", "com.slopos.imageviewer"),
            ("jpg", "com.slopos.imageviewer"),
            ("jpeg", "com.slopos.imageviewer"),
            ("gif", "com.slopos.imageviewer"),
            ("zip", "com.slopos.archiveutility"),
            ("pdf", "com.slopos.textedit"),
        ];
        for (ext, app) in defaults {
            self.associations.insert(
                ext.to_string(),
                FileAssociation {
                    extension: ext.to_string(),
                    default_app: app.to_string(),
                    user_override: None,
                },
            );
        }
    }

    pub fn register_bundle(&mut self, bundle: AppBundle) {
        self.bundles.insert(bundle.bundle_id.clone(), bundle);
    }

    /// Walk each search path for direct `*.app` children and register them.
    /// Bad/missing manifests are skipped with a warning (never panic).
    pub fn scan_applications(&mut self) {
        self.bundles.clear();
        let paths = self.search_paths.clone();
        for search in paths {
            let dir = Path::new(&search);
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let is_app = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".app"));
                if !is_app {
                    continue;
                }
                match crate::bundle::load_bundle(&path) {
                    Ok(bundle) => self.register_bundle(bundle),
                    Err(err) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = ?err,
                            "skipping invalid .app bundle"
                        );
                    }
                }
            }
        }
    }

    pub fn launch_app(&self, bundle_id: &str) -> Option<&AppBundle> {
        self.bundles.get(bundle_id)
    }

    pub fn app_for_extension(&self, extension: &str) -> Option<&str> {
        self.associations
            .get(extension)
            .map(|a| a.user_override.as_ref().unwrap_or(&a.default_app))
            .map(|s| s.as_str())
    }

    pub fn bundle_for_id(&self, bundle_id: &str) -> Option<&AppBundle> {
        self.bundles.get(bundle_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_applications_reads_app_dirs_from_disk() {
        let root = std::env::temp_dir().join(format!("rs_scan_apps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("Foo.app");
        let res = app.join("Resources");
        std::fs::create_dir_all(&res).unwrap();
        std::fs::write(
            res.join("Info.toml"),
            "bundle_id = \"com.slopos.foo\"\nname = \"Foo\"\nversion = \"0.1.0\"\nentrypoint = \"bin/foo\"\n",
        )
        .unwrap();

        let mut services = LaunchServices {
            bundles: HashMap::new(),
            associations: HashMap::new(),
            search_paths: vec![root.to_string_lossy().into_owned()],
        };
        services.scan_applications();

        let bundle = services
            .bundle_for_id("com.slopos.foo")
            .expect("registered");
        assert_eq!(bundle.name, "Foo");
        assert_eq!(bundle.entrypoint, "bin/foo");
        assert!(bundle.path.ends_with("Foo.app"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn new_includes_home_applications_path() {
        let services = LaunchServices::new();
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            let expected = format!("{home}/Applications");
            assert!(
                services.search_paths.iter().any(|p| p == &expected),
                "missing {expected} in {:?}",
                services.search_paths
            );
        }
        assert!(services.search_paths.iter().any(|p| p == "/Applications"));
    }
}
