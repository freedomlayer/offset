use crypto::identity::PublicKey;
use serde::{Deserialize, Deserializer};
use std::convert::TryFrom;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerConfig {
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
    port: u32,
    permission: Permission,
    public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub indexer: IndexerConfig,
    #[serde(deserialize_with = "validate_ports")]
    pub applications: HashMap<String, AppConfig>,
}

// TODO where to put this code? In `crypto` module or here?
impl<'a> Deserialize<'a> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<PublicKey, D::Error>
    where
        D: Deserializer<'a>
    {
        use serde::de::Error;
        String::deserialize(deserializer)
            .and_then(|string| ::base64::decode(&string).map_err(Error::custom))
            .and_then(|bytes| PublicKey::try_from(bytes.as_slice()).map_err(Error::custom))
    }
}

fn validate_ports<'a, D>(deserializer: D) -> Result<HashMap<String, AppConfig>, D::Error>
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
