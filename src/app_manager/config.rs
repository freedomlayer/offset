use crypto::identity::PublicKey;
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeSet, BTreeMap, HashMap};

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerConfig {
    #[serde(deserialize_with = "PublicKey::deserialize_base64")]
    pub public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

pub fn test1() {
    let text = r#"
    [indexer]
    public_key = "{key}"

    [applications]

    [applications.app0]
    port = 0
    permission = "all"
    public_key = "{key}"

    [applications.app1]
    port = 1
    permission = "all"
    public_key = "{key}"
        "#;

    let key = ::base64::encode(PublicKey::from(&[0u8; 32]).as_ref());
    let text = text.replace("{key}", &key);

    let config = ::toml::from_str::<Config>(&text);
    println!("{:?}", config);
}
