use std::convert::TryFrom;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::ToString;

use serde::de::{Deserialize, Error, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

pub mod ser_b64 {
    use super::*;

    pub fn serialize<T, S>(item: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: AsRef<[u8]>,
    {
        let base64_str = base64::encode_config(&item.as_ref(), URL_SAFE_NO_PAD);
        serializer.serialize_str(&base64_str)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: for<'t> TryFrom<&'t [u8]>,
    {
        struct ItemVisitor<T> {
            item: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for ItemVisitor<T>
        where
            T: for<'t> TryFrom<&'t [u8]>,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A bytes like item")
            }

            fn visit_str<E>(self, str_item: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let vec = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                    .map_err(|err| Error::custom(err.to_string()))?;
                T::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))
            }
        }

        let visitor = ItemVisitor { item: PhantomData };
        deserializer.deserialize_str(visitor)
    }
}

// =========================================================================

pub mod ser_string {
    use super::*;

    pub fn serialize<T, S>(item: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: ToString,
    {
        serializer.serialize_str(&item.to_string())
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: FromStr,
    {
        struct ItemVisitor<T> {
            item: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for ItemVisitor<T>
        where
            T: FromStr,
        {
            type Value = T;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A bytes like item")
            }

            fn visit_str<E>(self, str_item: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                str_item
                    .parse()
                    .map_err(|_| Error::custom("Failed to parse as string"))
            }
        }

        let visitor = ItemVisitor { item: PhantomData };
        deserializer.deserialize_str(visitor)
    }
}

// =========================================================================

/// A util for serializing HashMaps with keys that are not strings.
/// For example: JSON serialization does not allow keys that are not strings.
/// SerHashMap first converts the key to a base64 string, and only then serializes.
pub mod ser_map_b64_any {
    use super::*;

    pub fn serialize<S, K, V, M>(input_map: M, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize + AsRef<[u8]> + Eq + Hash,
        V: Serialize,
        M: IntoIterator<Item = (K, V)>,
    {
        let opt_length = None;
        let mut map = serializer.serialize_map(opt_length)?;
        for (k, v) in input_map.into_iter() {
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
}

// ===============================================================

pub mod ser_map_str_any {
    use super::*;

    pub fn serialize<'de, K, V, M, S>(input_map: M, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize + ToString + Eq + Hash,
        V: Serialize,
        M: IntoIterator<Item = (K, V)>,
    {
        let opt_length = None;
        let mut map = serializer.serialize_map(opt_length)?;
        for (k, v) in input_map.into_iter() {
            map.serialize_entry(&k.to_string(), &v)?;
        }
        map.end()
    }

    pub fn deserialize<'de, K, V, M, D>(deserializer: D) -> Result<M, D::Error>
    where
        D: Deserializer<'de>,
        K: Deserialize<'de> + FromStr + Eq + Hash,
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
            K: Deserialize<'de> + FromStr + Eq + Hash,
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
                    let k = k_string
                        .parse()
                        .map_err(|_| Error::custom(format!("Parse failed: {:?}", k_string)))?;

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
}

// ===============================================================

pub mod ser_map_str_str {
    use super::*;

    pub fn serialize<S, K, V, M>(input_map: M, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize + ToString + Eq + Hash,
        V: Serialize + ToString,
        M: IntoIterator<Item = (K, V)>,
    {
        let opt_length = None;
        let mut map = serializer.serialize_map(opt_length)?;
        for (k, v) in input_map.into_iter() {
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
}

// =========================================================================

pub mod ser_option_b64 {
    use super::*;

    pub fn serialize<T, S>(opt_item: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize + AsRef<[u8]>,
    {
        match opt_item {
            Some(item) => {
                let string_item = base64::encode_config(item.as_ref(), URL_SAFE_NO_PAD);
                serializer.serialize_some(&string_item)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
    {
        struct ItemVisitor<T> {
            item: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for ItemVisitor<T>
        where
            T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
        {
            type Value = Option<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("An option")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct B64Visitor<T> {
                    item: PhantomData<T>,
                }

                impl<'de, T> Visitor<'de> for B64Visitor<T>
                where
                    T: for<'t> TryFrom<&'t [u8]>,
                {
                    type Value = T;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("A bytes like item")
                    }

                    fn visit_str<E>(self, str_item: &str) -> Result<Self::Value, E>
                    where
                        E: Error,
                    {
                        let vec = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                            .map_err(|err| Error::custom(err.to_string()))?;
                        T::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))
                    }
                }

                let b64_visitor = B64Visitor { item: PhantomData };
                Ok(Some(deserializer.deserialize_string(b64_visitor)?))
            }
        }

        let visitor = ItemVisitor { item: PhantomData };
        deserializer.deserialize_option(visitor)
    }
}

// ============================================================================

pub mod ser_seq_b64 {
    use super::*;

    pub fn serialize<T, V, S>(input_vec: V, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize + AsRef<[u8]>,
        V: IntoIterator<Item = T>,
    {
        let items_iter = input_vec
            .into_iter()
            .map(|item| base64::encode_config(item.as_ref(), URL_SAFE_NO_PAD));
        serializer.collect_seq(items_iter)
    }

    pub fn deserialize<'de, T, V, D>(deserializer: D) -> Result<V, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
        V: Default + Extend<T>,
    {
        struct SeqVisitor<T, V> {
            item: PhantomData<T>,
            seq: PhantomData<V>,
        }

        impl<'de, T, V> Visitor<'de> for SeqVisitor<T, V>
        where
            T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
            V: Default + Extend<T>,
        {
            type Value = V;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A vector")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut res_vec: V = V::default();
                while let Some(str_item) = seq.next_element::<String>()? {
                    let item_vec: Vec<u8> = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                        .map_err(|err| Error::custom(err.to_string()))?;
                    let item =
                        T::try_from(&item_vec).map_err(|_| Error::custom("Length mismatch"))?;
                    res_vec.extend(Some(item));
                }
                Ok(res_vec)
            }
        }

        let visitor = SeqVisitor {
            item: PhantomData,
            seq: PhantomData,
        };
        deserializer.deserialize_seq(visitor)
    }
}

// ============================================================================

// TODO: Left here as a shim until ser_seq_b64 compiles.
pub mod ser_vec_b64 {
    use super::*;

    pub fn serialize<T, S>(input_vec: &Vec<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize + AsRef<[u8]>,
    {
        let items_iter = input_vec
            .iter()
            .map(|item| base64::encode_config(item.as_ref(), URL_SAFE_NO_PAD));
        serializer.collect_seq(items_iter)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
    {
        struct SeqVisitor<T> {
            item: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for SeqVisitor<T>
        where
            T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
        {
            type Value = Vec<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A vector")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut res_vec = Vec::new();
                while let Some(str_item) = seq.next_element::<String>()? {
                    let item_vec = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                        .map_err(|err| Error::custom(err.to_string()))?;
                    let item =
                        T::try_from(&item_vec).map_err(|_| Error::custom("Length mismatch"))?;
                    res_vec.push(item);
                }
                Ok(res_vec)
            }
        }

        let visitor = SeqVisitor { item: PhantomData };
        deserializer.deserialize_seq(visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /*
    #[allow(unused)]
    #[derive(Deserialize)]
    struct MyVecStruct {
        #[serde(with = "ser_seq_b64")]
        my_vec: Vec<[u8; 16]>,
    }
    */

    #[allow(unused)]
    #[derive(Deserialize)]
    struct MyVecStruct {
        #[serde(with = "ser_vec_b64")]
        my_vec: Vec<[u8; 16]>,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MyOptionB64Struct {
        #[serde(with = "ser_option_b64")]
        my_opt: Option<[u8; 16]>,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MyMapB64AnyStruct {
        #[serde(with = "ser_map_b64_any")]
        my_map: HashMap<[u8; 32], String>,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MyMapStrAnyStruct {
        #[serde(with = "ser_map_str_any")]
        my_map: HashMap<String, u64>,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MyMapStrStrStruct {
        #[serde(with = "ser_map_str_any")]
        my_map: HashMap<String, String>,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MySerStringStruct {
        #[serde(with = "ser_string")]
        my_string: String,
    }

    #[allow(unused)]
    #[derive(Serialize, Deserialize)]
    struct MySerB64Struct {
        #[serde(with = "ser_b64")]
        my_array: [u8; 32],
    }
}
