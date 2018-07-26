use crypto::identity::PublicKey;
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeSet, BTreeMap, HashMap};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IndexerConfig {
    #[serde(deserialize_with = "PublicKey::deserialize_base64")]
    pub public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
enum Permission {
    #[serde(rename = "view")]
    View,
    #[serde(rename = "communicate")]
    Communicate,
    #[serde(rename = "communicate+fund")]
    CommunicateAndFund,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(skip)]
    name: String,
    port: u32,
    permission: Permission,
    #[serde(deserialize_with = "PublicKey::deserialize_base64")]
    public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub indexer: IndexerConfig,
    #[serde(deserialize_with = "parse_applications_config")]
    pub applications: BTreeMap<u32, AppConfig>,
}

fn parse_applications_config<'a, D>(deserializer: D) -> Result<BTreeMap<u32, AppConfig>, D::Error>
where
    D: Deserializer<'a>
{
    use serde::de::Error;
    Deserialize::deserialize(deserializer).and_then(|apps: HashMap<String, AppConfig>| {
        let original_len = apps.values().len();
        let unique_len = apps.values().map(|app| app.port).collect::<BTreeSet<_>>().len();
        if original_len == unique_len {
            Ok(apps)
        } else {
            Err(Error::custom("applications must have unique ports"))
        }
    }).map(|mut apps| {
        apps.drain().map(|(k, mut v)| {
            v.name = k;
            (v.port, v)
        }).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::PUBLIC_KEY_LEN;

    #[test]
    fn test() {
        let key = PublicKey::from(&[0; PUBLIC_KEY_LEN]);

        let text = format!(r#"
[indexer]
public_key = "{key}"

[applications]

[applications.app0]
port = 0
permission = "view"
public_key = "{key}"

[applications.app1]
port = 1
permission = "communicate"
public_key = "{key}"

[applications.app2]
port = 2
permission = "communicate+fund"
public_key = "{key}"

[applications.app3]
port = 3
permission = "all"
public_key = "{key}"
        "#, key=::base64::encode(key.as_ref()));
        let config: Config = ::toml::from_str(&text).unwrap();

        assert_eq!(key, config.indexer.public_key);
        assert!(config.applications.values().all(|v| key == v.public_key));
        assert!(config.applications.iter().all(|(k, v)| *k == v.port));
        assert_eq!(config.applications[&0].permission, Permission::View);
        assert_eq!(config.applications[&1].permission, Permission::Communicate);
        assert_eq!(config.applications[&2].permission, Permission::CommunicateAndFund);
        assert_eq!(config.applications[&3].permission, Permission::All);
        assert_eq!(config.applications[&0].name, "app0");
        assert_eq!(config.applications[&1].name, "app1");
        assert_eq!(config.applications[&2].name, "app2");
        assert_eq!(config.applications[&3].name, "app3");
    }

    #[test]
    fn unique() {
        let key = PublicKey::from(&[0; PUBLIC_KEY_LEN]);

        let text = format!(r#"
[indexer]
public_key = "{key}"

[applications]

[applications.app0]
port = 0
permission = "view"
public_key = "{key}"

[applications.app1]
port = 0
permission = "communicate"
public_key = "{key}"
        "#, key=::base64::encode(key.as_ref()));

        ::toml::from_str::<Config>(&text).unwrap_err();
    }
}
