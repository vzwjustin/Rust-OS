//! GObject introspection repository matching `girepository/girepository.h`.
//!
//! Loads typelibs from search paths and resolves [class@GIRepository.BaseInfo]
//! records by namespace/name. Binary loading is stubbed; an in-memory typelib
//! registry backs `require` and `find_by_name` for now.

use crate::error::Error;
use crate::gibaseinfo::BaseInfo;
use crate::gitypelib::Typelib;
use crate::prelude::*;
use crate::quark::{quark_from_static_string, Quark};
use crate::stdio::read_file_bytes;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Flags controlling typelib loading (`GIRepositoryLoadFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct RepositoryLoadFlags(pub u32);

impl RepositoryLoadFlags {
    /// No flags set.
    pub const NONE: Self = Self(0);
    /// Ignore preload libraries.
    pub const LAZY: Self = Self(1 << 0);
    /// Allow fallback to an older typelib version.
    pub const ALLOW_FALLBACK: Self = Self(1 << 1);
}

/// Error quark for [class@GIRepository.Repository] (`GI_REPOSITORY_ERROR`).
pub fn repository_error_quark() -> Quark {
    quark_from_static_string(Some("g-irepository-error-quark"))
}

/// Repository error codes (`GIRepositoryError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositoryError {
    /// The typelib could not be found.
    TypelibNotFound,
    /// The namespace did not match.
    NamespaceMismatch,
    /// The typelib version conflicted with the request.
    NamespaceVersionConflict,
    /// A required shared library was not found.
    LibraryNotFound,
}

impl RepositoryError {
    /// Convert to a human-readable message.
    pub fn message(self) -> &'static str {
        match self {
            Self::TypelibNotFound => "typelib not found",
            Self::NamespaceMismatch => "namespace mismatch",
            Self::NamespaceVersionConflict => "namespace version conflict",
            Self::LibraryNotFound => "library not found",
        }
    }
}

/// Global in-memory typelib registry keyed by `(namespace, version)`.
static TYPELIB_REGISTRY: Mutex<BTreeMap<(String, String), Arc<Typelib>>> =
    Mutex::new(BTreeMap::new());

/// Register a typelib in the global in-memory registry (test / bootstrap helper).
pub fn register_typelib(typelib: Arc<Typelib>) {
    let key = (typelib.namespace().to_owned(), typelib.version().to_owned());
    TYPELIB_REGISTRY.lock().insert(key, typelib);
}

/// Clear the global typelib registry (tests only).
#[cfg(test)]
pub fn clear_typelib_registry() {
    TYPELIB_REGISTRY.lock().clear();
}

/// The GObject introspection repository (`GIRepository`).
#[derive(Debug, Default)]
pub struct Repository {
    search_paths: Vec<String>,
    library_paths: Vec<String>,
    loaded: BTreeMap<String, Arc<Typelib>>,
}

impl Repository {
    /// Create a new repository (`gi_repository_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the process-default repository (`gi_repository_dup_default`).
    pub fn default_repository() -> Arc<Mutex<Self>> {
        static DEFAULT: spin::Once<Arc<Mutex<Repository>>> = spin::Once::new();
        DEFAULT
            .call_once(|| Arc::new(Mutex::new(Repository::new())))
            .clone()
    }

    /// Prepend a typelib search path (`gi_repository_prepend_search_path`).
    pub fn prepend_search_path(&mut self, path: impl Into<String>) {
        self.search_paths.insert(0, path.into());
    }

    /// Append a typelib search path.
    pub fn append_search_path(&mut self, path: impl Into<String>) {
        self.search_paths.push(path.into());
    }

    /// Returns the configured search paths.
    pub fn search_paths(&self) -> &[String] {
        &self.search_paths
    }

    /// Prepend a shared-library search path (`gi_repository_prepend_library_path`).
    pub fn prepend_library_path(&mut self, path: impl Into<String>) {
        self.library_paths.insert(0, path.into());
    }

    /// Require a namespace typelib (`gi_repository_require`).
    ///
    /// Resolves from the in-memory global registry first, then searches
    /// `search_paths` for `{namespace}-{version}.typelib` on disk.
    pub fn require(
        &mut self,
        namespace: &str,
        version: Option<&str>,
        _flags: RepositoryLoadFlags,
    ) -> Result<Arc<Typelib>, Error> {
        let version = version.unwrap_or("0.0");
        let key = (namespace.to_owned(), version.to_owned());

        if let Some(tl) = self.loaded.get(namespace) {
            if tl.version() == version {
                return Ok(tl.ref_());
            }
        }

        let registry = TYPELIB_REGISTRY.lock();
        if let Some(tl) = registry.get(&key) {
            let tl = tl.ref_();
            self.loaded.insert(namespace.to_owned(), tl.ref_());
            return Ok(tl);
        }

        // Accept any version if an unversioned entry exists.
        for ((ns, ver), tl) in registry.iter() {
            if ns == namespace && (version == "0.0" || ver == version) {
                let tl = tl.ref_();
                self.loaded.insert(namespace.to_owned(), tl.ref_());
                return Ok(tl);
            }
        }
        drop(registry);

        if let Some(tl) = self.load_typelib_from_search_paths(namespace, version)? {
            self.loaded.insert(namespace.to_owned(), tl.ref_());
            return Ok(tl);
        }

        Err(Error::new(
            repository_error_quark(),
            RepositoryError::TypelibNotFound as i32,
            format!("Typelib `{namespace}-{version}` not found"),
        ))
    }

    fn load_typelib_from_search_paths(
        &self,
        namespace: &str,
        version: &str,
    ) -> Result<Option<Arc<Typelib>>, Error> {
        let candidates = if version == "0.0" {
            alloc::vec![
                format!("{namespace}.typelib"),
                format!("{namespace}-{version}.typelib"),
            ]
        } else {
            alloc::vec![format!("{namespace}-{version}.typelib")]
        };

        for search_path in &self.search_paths {
            for candidate in &candidates {
                let path = format!("{search_path}/{candidate}");
                let Some(data) = read_file_bytes(&path) else {
                    continue;
                };
                let tl = match Typelib::from_bytes(&data) {
                    Ok(tl) => tl,
                    Err(err) => {
                        return Err(Error::new(
                            repository_error_quark(),
                            RepositoryError::TypelibNotFound as i32,
                            format!("Failed to parse typelib `{}`: {}", path, err.message()),
                        ));
                    }
                };
                if tl.namespace() != namespace {
                    return Err(Error::new(
                        repository_error_quark(),
                        RepositoryError::NamespaceMismatch as i32,
                        format!(
                            "Typelib `{}` namespace `{}` does not match `{}`",
                            path,
                            tl.namespace(),
                            namespace
                        ),
                    ));
                }
                if version != "0.0" && tl.version() != version {
                    continue;
                }
                return Ok(Some(tl));
            }
        }

        Ok(None)
    }

    /// Find an info node by namespace and name (`gi_repository_find_by_name`).
    pub fn find_by_name(&self, namespace: &str, name: Option<&str>) -> Option<Arc<BaseInfo>> {
        let name = name?;
        let tl = self.loaded.get(namespace)?;
        tl.find_by_name(name)
    }

    /// Returns loaded namespaces.
    pub fn loaded_namespaces(&self) -> Vec<String> {
        self.loaded.keys().cloned().collect()
    }
}

/// Bootstrap a built-in `Test` namespace typelib for unit tests.
#[cfg(test)]
pub fn bootstrap_test_namespace() -> Arc<Typelib> {
    let mut entries = BTreeMap::new();
    let enum_info = EnumInfo::new(
        "TestEnum",
        "Test",
        &[("TEST_ZERO", 0, "zero"), ("TEST_ONE", 1, "one")],
    );
    register_entry(&mut entries, enum_info.base().ref_());
    let tl = Typelib::new_in_memory("Test", "1.0", entries);
    register_typelib(tl.ref_());
    tl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_search_paths_prepend_append() {
        let mut repo = Repository::new();
        repo.append_search_path("/usr/share/gir-1.0");
        repo.prepend_search_path("/custom/gir");
        assert_eq!(repo.search_paths()[0], "/custom/gir");
        assert_eq!(repo.search_paths()[1], "/usr/share/gir-1.0");
    }

    #[test]
    fn repository_require_and_find_by_name() {
        clear_typelib_registry();
        let tl = bootstrap_test_namespace();
        let mut repo = Repository::new();
        let loaded = repo
            .require("Test", Some("1.0"), RepositoryLoadFlags::NONE)
            .expect("typelib");
        assert!(Arc::ptr_eq(&loaded, &tl));
        let info = repo
            .find_by_name("Test", Some("TestEnum"))
            .expect("enum info");
        assert_eq!(info.name(), "TestEnum");
        assert_eq!(info.info_type(), InfoType::Enum);
    }

    #[test]
    fn repository_require_from_search_path() {
        clear_typelib_registry();
        let header = crate::gitypelib::TypeLibHeader {
            major_version: 4,
            minor_version: 0,
            n_entries: 1,
            n_local_entries: 1,
            directory: crate::gitypelib::TYPELIB_HEADER_SIZE as u32,
            size: 0,
        };
        let bytes = crate::gitypelib::build_test_typelib_bytes_with_entries(
            &header,
            "DiskTest",
            "3.0",
            &[("DiskEnum", 5)],
        );

        let dir =
            std::env::temp_dir().join(format!("glib-native-typelib-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("DiskTest-3.0.typelib");
        std::fs::write(&path, &bytes).expect("write typelib");

        let mut repo = Repository::new();
        repo.append_search_path(dir.to_string_lossy().into_owned());
        let loaded = repo
            .require("DiskTest", Some("3.0"), RepositoryLoadFlags::NONE)
            .expect("disk typelib");
        assert_eq!(loaded.namespace(), "DiskTest");
        assert_eq!(loaded.version(), "3.0");
        let info = repo
            .find_by_name("DiskTest", Some("DiskEnum"))
            .expect("enum");
        assert_eq!(info.info_type(), InfoType::Enum);

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }
}
