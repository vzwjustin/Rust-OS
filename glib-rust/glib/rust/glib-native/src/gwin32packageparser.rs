//! gwin32packageparser matching `gio/gwin32packageparser.c`.
//!
//! Parses UWP (Universal Windows Platform) package manifests. The C code
//! enumerates installed packages via WinRT; this port provides a simple
//! XML-ish parser for `AppxManifest.xml` elements without external crates.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A single UWP application entry from a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageApplication {
    pub app_id: String,
    pub display_name: String,
    pub description: String,
    pub logo: String,
    pub executable: String,
    pub entry_point: String,
}

/// Parsed UWP package manifest state.
#[derive(Debug)]
pub struct PackageParser {
    display_name: Mutex<Option<String>>,
    app_id: Mutex<Option<String>>,
    executable: Mutex<Option<String>>,
    applications: Mutex<Vec<PackageApplication>>,
    package_name: Mutex<Option<String>>,
    package_family_name: Mutex<Option<String>>,
}

impl PackageParser {
    pub fn new() -> Self {
        Self {
            display_name: Mutex::new(None),
            app_id: Mutex::new(None),
            executable: Mutex::new(None),
            applications: Mutex::new(Vec::new()),
            package_name: Mutex::new(None),
            package_family_name: Mutex::new(None),
        }
    }

    /// Parses manifest XML (or raw data) and populates parser state.
    ///
    /// Mirrors manifest parsing in `g_win32_package_parser_enum_packages`.
    pub fn parse_manifest(&self, xml_or_data: &str) -> Result<(), String> {
        *self.display_name.lock() = extract_element_text(xml_or_data, "DisplayName");
        *self.package_name.lock() = extract_attr_on_tag(xml_or_data, "Identity", "Name");
        *self.package_family_name.lock() =
            extract_attr_on_tag(xml_or_data, "Identity", "Publisher");

        let apps = parse_application_elements(xml_or_data);
        if apps.is_empty() {
            return Err("no Application elements found".to_string());
        }

        *self.applications.lock() = apps.clone();
        if let Some(first) = apps.first() {
            *self.app_id.lock() = Some(first.app_id.clone());
            *self.executable.lock() = Some(first.executable.clone());
            if self.display_name.lock().is_none() {
                *self.display_name.lock() = Some(first.display_name.clone());
            }
        }

        Ok(())
    }

    pub fn get_display_name(&self) -> Option<String> {
        self.display_name.lock().clone()
    }

    pub fn get_app_id(&self) -> Option<String> {
        self.app_id.lock().clone()
    }

    pub fn get_executable(&self) -> Option<String> {
        self.executable.lock().clone()
    }

    pub fn get_applications(&self) -> Vec<PackageApplication> {
        self.applications.lock().clone()
    }

    pub fn package_name(&self) -> Option<String> {
        self.package_name.lock().clone()
    }

    pub fn package_family_name(&self) -> Option<String> {
        self.package_family_name.lock().clone()
    }
}

impl Default for PackageParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Standalone helper — parses `<Application>` elements from manifest XML.
pub fn parse_manifest(xml: &str) -> Vec<PackageApplication> {
    parse_application_elements(xml)
}

/// Callback type for package enumeration (`GWin32PackageParserCallback`).
pub type PackageCallback = dyn FnMut(&ParsedPackage) -> bool;

/// Package summary passed to enumeration callbacks.
#[derive(Debug, Clone)]
pub struct ParsedPackage {
    pub package_name: String,
    pub display_name: String,
    pub app_user_model_id: String,
    pub applications: Vec<PackageApplication>,
}

/// Enumerates packages, invoking `callback` for each (stub registry).
pub fn enum_packages<F>(packages: &[ParsedPackage], mut callback: F)
where
    F: FnMut(&ParsedPackage) -> bool,
{
    for pkg in packages {
        if !callback(pkg) {
            break;
        }
    }
}

/// Converts an HRESULT to a simplified GIO error code.
pub fn hresult_to_io_error(hresult: i32) -> IoError {
    if hresult == 0 {
        return IoError::None;
    }
    match hresult & 0xFFFF {
        2 => IoError::NotFound,
        3 => IoError::PathNotFound,
        5 => IoError::AccessDenied,
        14 => IoError::OutOfMemory,
        87 => IoError::InvalidArgument,
        183 => IoError::Exists,
        _ => IoError::Failed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    None,
    NotFound,
    PathNotFound,
    AccessDenied,
    OutOfMemory,
    InvalidArgument,
    Exists,
    Failed,
}

fn parse_application_elements(xml: &str) -> Vec<PackageApplication> {
    let mut apps = Vec::new();
    let mut pos = 0;

    while let Some(rel) = xml[pos..].find("<Application") {
        let abs_start = pos + rel;
        let after_tag = &xml[abs_start + "<Application".len()..];
        if after_tag.starts_with('s') {
            pos = abs_start + 1;
            continue;
        }
        let Some(tag_end_rel) = xml[abs_start..].find('>') else {
            break;
        };
        let tag_content = &xml[abs_start..abs_start + tag_end_rel];
        if !tag_content.starts_with("<Application ") && tag_content != "<Application" {
            pos = abs_start + tag_end_rel + 1;
            continue;
        }

        apps.push(PackageApplication {
            app_id: extract_attr(tag_content, "Id").unwrap_or_default(),
            display_name: extract_attr(tag_content, "DisplayName").unwrap_or_default(),
            description: extract_attr(tag_content, "Description").unwrap_or_default(),
            logo: extract_attr(tag_content, "Square150x150Logo")
                .or_else(|| extract_attr(tag_content, "Logo"))
                .unwrap_or_default(),
            executable: extract_attr(tag_content, "Executable").unwrap_or_default(),
            entry_point: extract_attr(tag_content, "EntryPoint").unwrap_or_default(),
        });

        pos = abs_start + tag_end_rel + 1;
    }

    apps
}

fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr_name);
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn extract_attr_on_tag(xml: &str, tag_name: &str, attr_name: &str) -> Option<String> {
    let open = format!("<{tag_name}");
    let start = xml.find(&open)?;
    let tag_end = xml[start..].find('>')? + start;
    extract_attr(&xml[start..tag_end], attr_name)
}

fn extract_element_text(xml: &str, element: &str) -> Option<String> {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let text = xml[start..end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"
        <Package>
          <Identity Name="MyPackage" Publisher="CN=Publisher" Version="1.0.0.0"/>
          <Properties>
            <DisplayName>My Package</DisplayName>
          </Properties>
          <Applications>
            <Application Id="App" DisplayName="Main App" Executable="app.exe" EntryPoint="App">
            </Application>
            <Application Id="App2" DisplayName="Secondary" Executable="app2.exe">
            </Application>
          </Applications>
        </Package>
    "#;

    #[test]
    fn test_package_parser_parse_manifest() {
        let parser = PackageParser::new();
        parser.parse_manifest(SAMPLE_MANIFEST).unwrap();
        assert_eq!(parser.get_display_name(), Some("My Package".to_string()));
        assert_eq!(parser.get_app_id(), Some("App".to_string()));
        assert_eq!(parser.get_executable(), Some("app.exe".to_string()));
        assert_eq!(parser.get_applications().len(), 2);
        assert_eq!(parser.package_name(), Some("MyPackage".to_string()));
    }

    #[test]
    fn test_parse_manifest_helper() {
        let apps = parse_manifest(SAMPLE_MANIFEST);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].app_id, "App");
        assert_eq!(apps[0].executable, "app.exe");
        assert_eq!(apps[1].display_name, "Secondary");
    }

    #[test]
    fn test_parse_manifest_empty_fails() {
        let parser = PackageParser::new();
        assert!(parser.parse_manifest("<Package></Package>").is_err());
    }

    #[test]
    fn test_enum_packages() {
        let packages = vec![ParsedPackage {
            package_name: "Pkg".to_string(),
            display_name: "Pkg".to_string(),
            app_user_model_id: "Pkg!App".to_string(),
            applications: vec![],
        }];
        let mut count = 0;
        enum_packages(&packages, |_| {
            count += 1;
            true
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn test_hresult_to_io_error() {
        assert_eq!(hresult_to_io_error(0), IoError::None);
        assert_eq!(hresult_to_io_error(0x80070002u32 as i32), IoError::NotFound);
        assert_eq!(
            hresult_to_io_error(0x80070005u32 as i32),
            IoError::AccessDenied
        );
    }
}
