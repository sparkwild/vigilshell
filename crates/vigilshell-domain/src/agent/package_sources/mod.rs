//! 来源内容处理为纯规则，所有远端读取、密钥核验和提交归 SSH 适配器。
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
type Result<T> = AgentResult<T>;
mod validation;
pub use validation::*;
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn managed(path: &Path) -> bool {
    path.file_name()
        .and_then(|p| p.to_str())
        .is_some_and(|p| p.starts_with("vigilshell-managed-"))
}
pub fn toggle(path: &Path, text: &str, enabled: bool) -> Result<String> {
    let rpm = path.extension().is_some_and(|s| s == "repo");
    if rpm || path.extension().is_some_and(|s| s == "sources") {
        let field = if rpm { "enabled" } else { "Enabled" };
        let setting = if rpm {
            format!("enabled={}\n", u8::from(enabled))
        } else {
            format!("Enabled: {}\n", if enabled { "yes" } else { "no" })
        };
        let mut output = String::new();
        let mut section = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if (rpm && trimmed.starts_with('[')) || (!rpm && trimmed.is_empty()) {
                if section {
                    output.push_str(&setting);
                }
                section = false;
            }
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                section = true;
            }
            let key = line
                .split_once(if rpm { '=' } else { ':' })
                .map(|(k, _)| k.trim());
            if key.is_some_and(|k| k.eq_ignore_ascii_case(field)) {
                continue;
            }
            output.push_str(line);
            output.push('\n');
        }
        if section {
            output.push_str(&setting);
        }
        Ok(output)
    } else {
        let mut output = String::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            let candidate = trimmed
                .strip_prefix("# vigilshell-disabled ")
                .unwrap_or_else(|| {
                    trimmed
                        .strip_prefix('#')
                        .map(str::trim_start)
                        .unwrap_or(trimmed)
                });
            if candidate.starts_with("deb ") || candidate.starts_with("deb-src ") {
                if !enabled {
                    output.push_str("# vigilshell-disabled ");
                }
                output.push_str(candidate);
            } else {
                output.push_str(line);
            }
            output.push('\n');
        }
        Ok(output)
    }
}
pub struct Template {
    pub name: String,
    pub key: String,
    pub path: String,
    pub key_path: String,
    pub text: String,
}
pub fn template(
    definition: &PackageSourceDefinition,
    selected_key: Option<&str>,
) -> Result<Template> {
    definition.validate()?;
    let (name, key, path, key_path, text) = match definition {
        PackageSourceDefinition::Apt {
            name,
            url,
            suite,
            components,
            public_key,
        } => {
            let key_path = selected_key.map(str::to_owned).unwrap_or_else(|| {
                format!(
                    "/usr/share/keyrings/vigilshell-managed-{name}-{}.asc",
                    digest(public_key.as_bytes())
                )
            });
            let text = format!(
                "Types: deb\nURIs: {url}\nSuites: {suite}\nComponents: {}\nSigned-By: {key_path}\nEnabled: no\n",
                components.join(" ")
            );
            (
                name,
                public_key,
                format!("/etc/apt/sources.list.d/vigilshell-managed-{name}.sources"),
                key_path,
                text,
            )
        }
        PackageSourceDefinition::Rpm {
            name,
            url,
            public_key,
        } => {
            let key_path = selected_key.map(str::to_owned).unwrap_or_else(|| {
                format!(
                    "/etc/pki/rpm-gpg/vigilshell-managed-{name}-{}.asc",
                    digest(public_key.as_bytes())
                )
            });
            let text = format!(
                "[vigilshell-managed-{name}]\nname={name}\nbaseurl={url}\ngpgcheck=1\nsslverify=1\ngpgkey=file://{key_path}\nenabled=0\n"
            );
            (
                name,
                public_key,
                format!("/etc/yum.repos.d/vigilshell-managed-{name}.repo"),
                key_path,
                text,
            )
        }
    };
    Ok(Template {
        name: name.clone(),
        key: key.clone(),
        path,
        key_path,
        text,
    })
}
pub fn parse(path: &Path, text: &str, key: String) -> Result<(PackageSourceDefinition, String)> {
    let rpm = path.extension().is_some_and(|s| s == "repo");
    let file = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid_package_source")?;
    let name = file
        .strip_prefix("vigilshell-managed-")
        .and_then(|s| s.strip_suffix(if rpm { ".repo" } else { ".sources" }))
        .ok_or("package_source_contract_unsupported")?;
    let mut fields = BTreeMap::new();
    let mut sections = 0;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if rpm && line.starts_with('[') {
            sections += 1;
            if line != format!("[vigilshell-managed-{name}]") || sections > 1 {
                return Err("package_source_contract_unsupported");
            }
            continue;
        }
        let (key, value) = line
            .split_once(if rpm { '=' } else { ':' })
            .ok_or("package_source_contract_unsupported")?;
        let key = key.trim();
        let value = value.trim();
        let allowed = if rpm {
            &[
                "name",
                "baseurl",
                "gpgcheck",
                "sslverify",
                "gpgkey",
                "enabled",
            ][..]
        } else {
            &[
                "Types",
                "URIs",
                "Suites",
                "Components",
                "Signed-By",
                "Enabled",
            ][..]
        };
        if !allowed.contains(&key) || fields.insert(key, value).is_some() {
            return Err("package_source_contract_unsupported");
        }
    }
    let get = |name: &str| {
        fields
            .get(name)
            .copied()
            .ok_or("package_source_contract_unsupported")
    };
    let key_path = if rpm {
        if sections != 1
            || get("name")? != name
            || get("gpgcheck")? != "1"
            || get("sslverify")? != "1"
            || !matches!(get("enabled")?, "0" | "1")
        {
            return Err("package_source_contract_unsupported");
        }
        get("gpgkey")?
            .strip_prefix("file://")
            .ok_or("package_source_contract_unsupported")?
    } else {
        if get("Types")? != "deb" || !matches!(get("Enabled")?, "yes" | "no") {
            return Err("package_source_contract_unsupported");
        }
        get("Signed-By")?
    };
    let prefix = format!(
        "{}/vigilshell-managed-{name}",
        if rpm {
            "/etc/pki/rpm-gpg"
        } else {
            "/usr/share/keyrings"
        }
    );
    let suffix = key_path
        .strip_prefix(&prefix)
        .ok_or("package_source_contract_unsupported")?;
    if suffix != ".asc"
        && !suffix
            .strip_prefix('-')
            .and_then(|s| s.strip_suffix(".asc"))
            .is_some_and(valid_content_hash)
    {
        return Err("package_source_contract_unsupported");
    }
    let definition = if rpm {
        PackageSourceDefinition::Rpm {
            name: name.into(),
            url: get("baseurl")?.into(),
            public_key: key,
        }
    } else {
        PackageSourceDefinition::Apt {
            name: name.into(),
            url: get("URIs")?.into(),
            suite: get("Suites")?.into(),
            components: get("Components")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            public_key: key,
        }
    };
    definition.validate()?;
    Ok((definition, key_path.into()))
}
pub fn source_path(path: &str) -> bool {
    let p = Path::new(path);
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    path == "/etc/apt/sources.list"
        || (p.parent() == Some(Path::new("/etc/apt/sources.list.d"))
            && (name.ends_with(".list") || name.ends_with(".sources")))
        || (p.parent() == Some(Path::new("/etc/yum.repos.d")) && name.ends_with(".repo"))
}
pub fn key_paths(text: &str) -> Vec<String> {
    let mut keys = std::collections::BTreeSet::new();
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '[' | ']')) {
        let v = word
            .strip_prefix("signed-by=")
            .or_else(|| word.strip_prefix("gpgkey="))
            .unwrap_or(word);
        let v = v.strip_prefix("file://").unwrap_or(v);
        if key_path(v) {
            keys.insert(v.to_owned());
        }
    }
    keys.into_iter().collect()
}
pub fn key_path(path: &str) -> bool {
    [
        "/usr/share/keyrings/",
        "/usr/share/distribution-gpg-keys/",
        "/etc/apt/trusted.gpg.d/",
        "/etc/apt/keyrings/",
        "/etc/pki/rpm-gpg/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || path == "/etc/apt/trusted.gpg"
}
pub fn validate_document(path: &Path, text: &str) -> Result<()> {
    if text.len() > 1024 * 1024 || text.contains('\0') || text.contains("PRIVATE KEY") {
        return Err("invalid_package_source");
    }
    if path.extension().is_some_and(|s| s == "repo") {
        validate_rpm(text)
    } else {
        if !text.lines().any(|l| {
            let l = l.trim_start().trim_start_matches('#').trim_start();
            l.starts_with("deb ")
                || l.starts_with("deb-src ")
                || l.starts_with("Types:")
                || l.starts_with("vigilshell-disabled deb")
        }) {
            return Err("invalid_package_source");
        }
        validate(text)
    }
}
pub fn identity(path: &Path, text: &str, keys: &[(String, String)]) -> Result<String> {
    validate_document(path, text)?;
    let canonical = if managed(path) {
        toggle(path, text, true)?
    } else {
        text.into()
    };
    let mut material = format!("{}\0{canonical}", path.display());
    for (key, hash) in keys {
        material.push_str(&format!("\0{key}\0{hash}"));
    }
    Ok(digest(material.as_bytes()))
}
