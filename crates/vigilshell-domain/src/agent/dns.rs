//! Static resolver changes are typed and cannot carry arbitrary configuration text.
use super::*;
use std::net::IpAddr;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DnsConfiguration {
    pub servers: Vec<String>,
    pub search_domains: Vec<String>,
}
impl DnsConfiguration {
    pub fn validate(&self) -> AgentResult<()> {
        if self.servers.is_empty()
            || self.servers.len() > 3
            || self.search_domains.len() > 6
            || self.search_domains.iter().map(String::len).sum::<usize>() > 250
        {
            return Err("invalid_dns_configuration");
        }
        for (index, value) in self.servers.iter().enumerate() {
            let address = value
                .parse::<IpAddr>()
                .map_err(|_| "invalid_dns_configuration")?;
            if address.is_unspecified()
                || address.is_multicast()
                || self.servers[..index].contains(value)
            {
                return Err("invalid_dns_configuration");
            }
        }
        for domain in &self.search_domains {
            if domain.len() > 253
                || domain.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || !label.starts_with(|c: char| c.is_ascii_alphanumeric())
                        || !label.ends_with(|c: char| c.is_ascii_alphanumeric())
                        || !label
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
            {
                return Err("invalid_dns_configuration");
            }
        }
        Ok(())
    }
    pub fn replace(&self, previous: &str) -> AgentResult<String> {
        self.validate()?;
        if previous.len() > 64 * 1024 || previous.contains('\0') {
            return Err("invalid_dns_configuration");
        }
        let mut text = String::new();
        for line in previous.lines() {
            match line.split_whitespace().next() {
                Some("nameserver" | "search" | "domain") => continue,
                Some("options" | "sortlist") | None => {}
                Some(first) if first.starts_with(['#', ';']) => {}
                _ => return Err("resolver_configuration_unsupported"),
            }
            text.push_str(line);
            text.push('\n');
        }
        for server in &self.servers {
            text.push_str(&format!("nameserver {server}\n"));
        }
        if !self.search_domains.is_empty() {
            text.push_str(&format!("search {}\n", self.search_domains.join(" ")));
        }
        Ok(text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_changes_preserve_options_without_injection() {
        let mut config = DnsConfiguration {
            servers: vec!["1.1.1.1".into()],
            search_domains: vec!["example.test".into()],
        };
        assert_eq!(
            config
                .replace("# owner\noptions timeout:2\nnameserver 8.8.8.8\n")
                .unwrap(),
            "# owner\noptions timeout:2\nnameserver 1.1.1.1\nsearch example.test\n"
        );
        config.search_domains = vec!["example\noptions debug".into()];
        assert!(config.validate().is_err());
        config.search_domains.clear();
        config.servers = vec!["0.0.0.0".into()];
        assert!(config.validate().is_err());
    }
}
