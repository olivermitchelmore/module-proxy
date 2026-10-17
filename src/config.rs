use axum::http;
use axum::http::Uri;
use axum::http::uri::{Authority, Scheme};
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::str::FromStr;
use thiserror::Error;
use tracing::debug;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to parse config")]
    InvalidFormat(#[from] toml::de::Error),
    #[error("origin {origin:?} has invalid url {url:?}")]
    InvalidOrigin {
        origin: String,
        url: String,
        #[source]
        source: UrlError,
    },
    #[error("host {host:?} is invalid")]
    InvalidHost {
        host: String,
        #[source]
        source: HostError,
    },
    #[error("host {host:?} is duplicate")]
    DuplicateHost {
        host: String,
    },
}

#[derive(Error, Debug)]
pub enum HostError {
    #[error("empty host")]
    EmptyHost,
    #[error("invalid host {host:?}")]
    InvalidHost {
        host: String,
        #[source]
        source: http::uri::InvalidUri,
    },
}

#[derive(Error, Debug)]
pub enum UrlError {
    #[error("invalid Scheme")]
    InvalidScheme {
        scheme: Scheme,
    },
    #[error("malformed URL")]
    MalformedUri {
        uri: String,
        #[source]
        source: http::uri::InvalidUri,
    },
    #[error("missing scheme")]
    MissingScheme,
    #[error("missing port")]
    MissingPort,
    #[error("missing host")]
    MissingHost,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    host: String,
    url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginList {
    origin: Vec<Origin>,
}

#[derive(Debug)]
pub struct Config {
    origins: HashMap<String, Uri>,
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Config, ConfigError> {
        let origin_list = toml::from_str::<OriginList>(text)?;
        let mut origins: HashMap<String, Uri> = HashMap::new();
        for origin in origin_list.origin {
            let validated_origin_uri =
                validate_uri(&origin.url).map_err(|e| ConfigError::InvalidOrigin {
                    origin: origin.host.clone(),
                    url: origin.url,
                    source: e,
                })?;
            let validated_host = validate_host(&origin.host).map_err(|e| ConfigError::InvalidHost {
                source: e,
                host: origin.host.clone(),
            })?;
            match origins.entry(validated_host) {
                Entry::Occupied(entry) => {
                    return Err(ConfigError::DuplicateHost {
                        host: entry.key().clone(),
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(validated_origin_uri);
                }
            }
        }
        Ok(Self {
            origins,
        })
    }
    pub fn get_origin(&self, host: &str) -> Option<&Uri> {
        let validated_host = match validate_host(host) {
            Ok(host) => host,
            Err(e) => {
                debug!(host = ?host, error = %e, "failed to validate host");
                return None;
            }
        };
        self.origins.get(&validated_host)
    }
}

pub fn validate_uri(uri: &str) -> Result<Uri, UrlError> {
    let uri = uri.parse::<Uri>().map_err(|e| UrlError::MalformedUri {
        uri: uri.to_owned(),
        source: e,
    })?;
    let scheme = uri.scheme().ok_or(UrlError::MissingScheme)?;
    if scheme != &Scheme::HTTP {
        return Err(UrlError::InvalidScheme {
            scheme: scheme.clone(),
        });
    }
    if let Some("") = uri.host() {
        return Err(UrlError::MissingHost);
    };
    uri.port().ok_or(UrlError::MissingPort)?;
    Ok(uri)
}

pub fn validate_host(host: &str) -> Result<String, HostError> {
    let authority = Authority::from_str(&host).map_err(|e| HostError::InvalidHost {
        host: host.to_owned(),
        source: e,
    })?;
    if authority.host() == "" {
        return Err(HostError::EmptyHost);
    }
    Ok(authority.host().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod config {
        use super::*;
        use std::assert_matches;
        #[test]
        fn creates_config_from_toml() {
            let text = r#"
            [[origin]]
            host = "example.com"
            url = "http://127.0.0.1:3000"
            "#;
            let config = Config::from_toml(text).unwrap();
            assert_eq!(
                config.origins.get("example.com"),
                Some(&Uri::from_str("http://127.0.0.1:3000").unwrap())
            );
        }

        #[test]
        fn invalid_origin_fails() {
            let text = r#"
            [[origin]]
            host = "example.com"
            url = "http://127.0.0.1"
            "#;
            assert!(matches!(Config::from_toml(text), Err(ConfigError::InvalidOrigin { .. })))
        }

        #[test]
        fn rejects_duplicate_host() {
            let text = r#"
            [[origin]]
            host = "example.com"
            url = "http://127.0.0.1:3000"
            [[origin]]
            host = "example.com"
            url = "http://127.0.0.1:3000"
            "#;
            assert!(matches!(Config::from_toml(text), Err(ConfigError::DuplicateHost { .. })))
        }

        #[test]
        fn rejects_malformed_toml() {
            let text = r#"
            [[origin]]
            host "example.com"
            url = "http://127.0.0.1:3000"
            [[origin]]
            host = "something.com"
            url = "http://127.0.0.1:3000"
            "#;
            assert_matches!(Config::from_toml(text), Err(ConfigError::InvalidFormat { .. }));
        }

        #[test]
        fn reject_incorrect_table_name() {
            let text = r#"
            [[incorrect]]
            host = "example.com"
            url = "http://127.0.0.1:3000"
            "#;
            assert_matches!(Config::from_toml(text), Err(ConfigError::InvalidFormat { .. }));
        }

        #[test]
        fn reject_incorrect_key_value_pairs() {
            let text = r#"
            [[origin]]
            incorrect = "example.com"
            url = "http://127.0.0.1:3000"
            "#;
            assert_matches!(Config::from_toml(text), Err(ConfigError::InvalidFormat { .. }));
        }
    }
    mod uri {
        use super::*;
        #[test]
        fn accepts_valid_uri() {
            let uri = &"http://127.0.0.1:3000/".to_string();
            assert!(matches!(validate_uri(uri), Ok(_)));
        }
        #[test]
        fn rejects_uri_without_scheme() {
            let uri = &"127.0.0.1:3000".to_string();
            assert!(matches!(validate_uri(uri), Err(UrlError::MissingScheme)));
        }

        #[test]
        fn rejects_invalid_scheme() {
            let uri = &"aaaa://127.0.0.1:3000/".to_string();
            assert!(matches!(validate_uri(uri), Err(UrlError::InvalidScheme { .. })));
        }

        #[test]
        fn rejects_malformed_url() {
            let uri = &"http://127.0 .0.1:3000/".to_string();
            assert!(matches!(validate_uri(uri), Err(UrlError::MalformedUri { .. })));
        }

        #[test]
        fn rejects_empty_host() {
            let uri = &"http://:3000/".to_string();
            assert!(matches!(validate_uri(uri), Err(UrlError::MissingHost)));
        }
    }

    mod host {
        use super::*;
        #[test]
        fn accepts_valid_host() {
            let host = "example.com";
            assert_eq!(validate_host(host).unwrap(), "example.com")
        }

        #[test]
        fn rejects_empty_host() {
            let host = ":8000";
            assert!(matches!(validate_host(host), Err(HostError::EmptyHost)))
        }

        #[test]
        fn rejects_invalid_host() {
            let host = "exam ple.com";
            assert!(matches!(validate_host(host), Err(HostError::InvalidHost { .. })))
        }

        #[test]
        fn normalizes_upper_case_host() {
            let host = "EXamPLe.COm";
            assert_eq!(validate_host(host).unwrap(), "example.com")
        }

        #[test]
        fn strips_port() {
            let host = "example.com:3000";
            assert_eq!(validate_host(host).unwrap(), "example.com")
        }
    }
}
