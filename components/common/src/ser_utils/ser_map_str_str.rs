use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::ToString;

use serde::de::{Deserialize, Error, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde::Deserializer;

pub fn serialize<S, K, V, M>(input_map: M, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Serialize + ToString + Eq + Hash,
    V: Serialize + ToString,
    M: IntoIterator<Item = (K, V)>,
{
    // TODO: A more efficient way to do this?
    let pairs: Vec<_> = input_map.into_iter().collect();
    let mut map = serializer.serialize_map(Some(pairs.len()))?;
    for (k, v) in pairs.into_iter() {
        map.serialize_entry(&k.to_string(), &v.to_string())?;
    }
    map.end()
}

pub fn deserialize<'de, K, V, M, D>(deserializer: D) -> Result<M, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + FromStr + Eq + Hash,
    V: Deserialize<'de> + FromStr,
    M: Default + Extend<(K, V)>,
{
    struct MapVisitor<K, V, M> {
        key: PhantomData<K>,
        value: PhantomData<V>,
        map: PhantomData<M>,
    }

    impl<'de, K, V, M> Visitor<'de> for MapVisitor<K, V, M>
    where
        K: Deserialize<'de> + FromStr + Eq + Hash,
        V: Deserialize<'de> + FromStr,
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

            while let Some((k_string, v_string)) = map.next_entry::<String, String>()? {
                let k = k_string
                    .parse()
                    .map_err(|_| Error::custom("Parse failed"))?;

                let v = v_string
                    .parse()
                    .map_err(|_| Error::custom("Parse failed"))?;

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
