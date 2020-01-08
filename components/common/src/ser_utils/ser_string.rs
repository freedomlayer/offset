use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::ToString;

use serde::de::{Error, Visitor};
use serde::ser::Serializer;
use serde::Deserializer;

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
