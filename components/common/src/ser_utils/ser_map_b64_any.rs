use std::convert::TryFrom;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::string::ToString;

use serde::de::{Deserialize, Error, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

// A util for serializing HashMaps with keys that are not strings.
// For example: JSON serialization does not allow keys that are not strings.
// SerHashMap first converts the key to a base64 string, and only then serializes.

pub fn serialize<S, K, V, M>(input_map: M, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Serialize + AsRef<[u8]> + Eq + Hash,
    V: Serialize,
    M: IntoIterator<Item = (K, V)>,
{
    // TODO: A more efficient way to do this?
    let pairs: Vec<_> = input_map.into_iter().collect();
    let mut map = serializer.serialize_map(Some(pairs.len()))?;
    for (k, v) in pairs.into_iter() {
        let string_k = base64::encode_config(k.as_ref(), URL_SAFE_NO_PAD);
        map.serialize_entry(&string_k, &v)?;
    }
    map.end()
}

pub fn deserialize<'de, D, K, V, M>(deserializer: D) -> Result<M, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + for<'t> TryFrom<&'t [u8]> + Eq + Hash,
    V: Deserialize<'de>,
    M: Default + Extend<(K, V)>,
{
    struct MapVisitor<K, V, M> {
        key: PhantomData<K>,
        value: PhantomData<V>,
        map: PhantomData<M>,
    }

    impl<'de, K, V, M> Visitor<'de> for MapVisitor<K, V, M>
    where
        K: Deserialize<'de> + for<'t> TryFrom<&'t [u8]> + Eq + Hash,
        V: Deserialize<'de>,
        M: Default + Extend<(K, V)>,
    {
        type Value = M;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("A map")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut res_map = M::default();
            while let Some((k_string, v)) = map.next_entry::<String, V>()? {
                let vec = base64::decode_config(&k_string, URL_SAFE_NO_PAD)
                    .map_err(|err| Error::custom(err.to_string()))?;
                let k = K::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))?;

                res_map.extend(Some((k, v)));
            }
            Ok(res_map)
        }
    }

    let visitor = MapVisitor {
        key: PhantomData,
        value: PhantomData,
        map: PhantomData,
    };
    deserializer.deserialize_map(visitor)
}
