use super::*;
pub fn validate(value: &str) -> Result<()> {
    for line in value.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let lower = line.to_ascii_lowercase();
        // 不解析并重新解释 apt 的全部语法；遇到弱化验证选项直接拒绝这个具体来源。
        if [
            "trusted",
            "allow-insecure",
            "allow-weak",
            "allow-downgrade-to-insecure",
            "check-valid-until",
            "check-date",
        ]
        .iter()
        .any(|s| lower.contains(s))
        {
            return Err("package_source_verification_override");
        }
        for word in line.split_whitespace().filter(|w| w.contains("://")) {
            if !(word.starts_with("https://") || word.starts_with("http://"))
                || word
                    .split("://")
                    .nth(1)
                    .unwrap_or("")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .contains('@')
            {
                return Err("package_source_transport_unsupported");
            }
        }
    }
    Ok(())
}

pub fn validate_rpm(text: &str) -> Result<()> {
    if text.len() > 1024 * 1024 || text.contains('\0') {
        return Err("invalid_package_source");
    }
    let mut sections = std::collections::BTreeSet::new();
    let mut fields = std::collections::BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if section.is_empty()
                || section.chars().any(char::is_whitespace)
                || !sections.insert(section)
            {
                return Err("package_source_contract_unsupported");
            }
            fields.clear();
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or("package_source_contract_unsupported")?;
        let key = key.trim().to_ascii_lowercase();
        if sections.is_empty()
            || key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !fields.insert(key.clone())
        {
            return Err("package_source_contract_unsupported");
        }
        if key == "enabled" && !matches!(value.trim(), "0" | "1" | "true" | "false" | "yes" | "no")
        {
            return Err("package_source_contract_unsupported");
        }
    }
    if sections.is_empty() {
        return Err("package_source_contract_unsupported");
    }
    Ok(())
}
