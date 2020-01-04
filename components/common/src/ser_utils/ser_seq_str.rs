use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::ToString;

use serde::de::{Deserialize, Error, SeqAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use serde::Deserializer;

pub fn serialize<T, V, S>(input_vec: V, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize + ToString,
    V: IntoIterator<Item = T>,
{
    let items_iter = input_vec.into_iter().map(|item| item.to_string());
    serializer.collect_seq(items_iter)
}

pub fn deserialize<'de, T, V, D>(deserializer: D) -> Result<V, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + FromStr,
    V: Default + Extend<T>,
{
    struct SeqVisitor<T, V> {
        item: PhantomData<T>,
        seq: PhantomData<V>,
    }

    impl<'de, T, V> Visitor<'de> for SeqVisitor<T, V>
    where
        T: Deserialize<'de> + FromStr,
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
                let item = T::from_str(&str_item).map_err(|_| Error::custom("Length mismatch"))?;
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
