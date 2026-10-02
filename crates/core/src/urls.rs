//! URLs shown to users and returned by the API. They're built from the
//! configured public URL, never from a request's `Host` header (NFR-SEC-001).

/// Builds URLs from `server.public_url`.
#[derive(Debug, Clone)]
pub struct Urls {
    /// Without a trailing `/`, e.g. `https://git.example.com`.
    base: String,
    host: String,
}

impl Urls {
    /// Fails with a message naming the setting if `public_url` isn't an `http(s)`
    /// URL with a host.
    pub fn new(public_url: &str) -> Result<Self, String> {
        let base = public_url.trim_end_matches('/');
        let rest =
            base.strip_prefix("https://").or_else(|| base.strip_prefix("http://")).ok_or_else(|| {
                format!("server.public_url must start with http:// or https://, got {public_url:?}")
            })?;
        let authority = rest.split('/').next().unwrap_or_default();
        let host = authority.rsplit_once(':').map_or(authority, |(host, _port)| host);
        if host.is_empty() {
            return Err(format!("server.public_url has no host: {public_url:?}"));
        }
        Ok(Self { base: base.to_owned(), host: host.to_owned() })
    }

    /// Whether people reach Klotho over HTTPS, which secure cookies need.
    pub fn is_https(&self) -> bool {
        self.base.starts_with("https://")
    }

    /// The repository's web page: `https://host/Owner/Repo`.
    pub fn html(&self, full_name: &str) -> String {
        format!("{}/{full_name}", self.base)
    }

    /// The HTTPS clone URL, with the display name and `.git` (FR-NAME-042).
    pub fn clone(&self, full_name: &str) -> String {
        format!("{}/{full_name}.git", self.base)
    }

    /// Any page on this site, e.g. `absolute("/-/register?invite=…")`.
    pub fn absolute(&self, path_and_query: &str) -> String {
        format!("{}{path_and_query}", self.base)
    }

    /// An API URL, e.g. for `Link` headers: `api("/users/x/repos")`.
    pub fn api(&self, path_and_query: &str) -> String {
        format!("{}/api/v1{path_and_query}", self.base)
    }

    /// The SSH clone URL. SSH itself arrives in Phase 4.
    pub fn ssh(&self, full_name: &str) -> String {
        format!("git@{}:{full_name}.git", self.host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_repository_urls() {
        let urls = Urls::new("https://git.example.com:8443/").unwrap();
        assert_eq!(urls.html("Alice/MyRepo"), "https://git.example.com:8443/Alice/MyRepo");
        assert_eq!(urls.clone("Alice/MyRepo"), "https://git.example.com:8443/Alice/MyRepo.git");
        assert_eq!(urls.ssh("Alice/MyRepo"), "git@git.example.com:Alice/MyRepo.git");
    }

    #[test]
    fn rejects_urls_without_scheme_or_host() {
        assert!(Urls::new("git.example.com").is_err());
        assert!(Urls::new("https://").is_err());
        assert!(Urls::new("ftp://x").is_err());
    }
}
