// Based on:
// - https://stackoverflow.com/questions/48782047/binary-deserialization-of-u8-128
// - https://github.com/serde-rs/json/issues/402
//

use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use base64::{self, URL_SAFE_NO_PAD};

use serde::de::{Deserialize, Deserializer, Error, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};

/// A util for serializing HashMaps with keys that are not strings.
/// For example: JSON serialization does not allow keys that are not strings.
/// SerHashMap first converts the key to a base64 string, and only then serializes.
pub trait SerHashMap<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

impl<'de, K, V> SerHashMap<'de> for HashMap<K, V>
where
    K: Serialize + Deserialize<'de> + AsRef<[u8]> + for<'t> TryFrom<&'t [u8]> + Eq + Hash,
    V: Serialize + Deserialize<'de>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self {
            let string_k = base64::encode_config(k.as_ref(), URL_SAFE_NO_PAD);
            map.serialize_entry(&string_k, v)?;
        }
        map.end()
    }

    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor<K, V> {
            key: PhantomData<K>,
            value: PhantomData<V>,
        }

        impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
        where
            K: Deserialize<'de> + for<'t> TryFrom<&'t [u8]> + Eq + Hash,
            V: Deserialize<'de>,
        {
            type Value = HashMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut res_map = HashMap::new();
                while let Some((k_string, v)) = map.next_entry::<String, V>()? {
                    let vec = base64::decode_config(&k_string, URL_SAFE_NO_PAD)
                        .map_err(|err| Error::custom(err.to_string()))?;
                    let k = K::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))?;

                    res_map.insert(k, v);
                }
                Ok(res_map)
            }
        }

        let visitor = MapVisitor {
            key: PhantomData,
            value: PhantomData,
        };
        deserializer.deserialize_map(visitor)
    }
}
