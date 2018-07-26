use crypto::identity::PublicKey;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerConfig {
    #[serde(deserialize_with = "from_base64")]
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
    #[serde(deserialize_with = "from_base64")]
    public_key: PublicKey,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub indexer: IndexerConfig,
    pub applications: HashMap<String, AppConfig>,
}

/// Code from: https://github.com/serde-rs/serde/issues/661
fn from_base64<'a, D>(deserializer: D) -> Result<PublicKey, D::Error>
    where D: ::serde::Deserializer<'a>
{
    use serde::de::Error;
    use serde::Deserialize;
    use std::convert::TryFrom;
    String::deserialize(deserializer)
        .and_then(|string| ::base64::decode(&string).map_err(Error::custom))
        .and_then(|bytes| PublicKey::try_from(bytes.as_slice()).map_err(Error::custom))
}

pub fn test1() {
    let text = r#"
    [indexer]
    public_key = "{key}"

    [applications]
        "#;

    let key = ::base64::encode(PublicKey::from(&[0u8; 32]).as_ref());
    let text = text.replace("{key}", &key);

    let config = ::toml::from_str::<Config>(&text);
    println!("{:?}", config);
}
