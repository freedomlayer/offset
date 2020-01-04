use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::string::ToString;

use serde::de::{Deserialize, Error, SeqAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

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
                let item = T::try_from(&item_vec).map_err(|_| Error::custom("Length mismatch"))?;
                res_vec.push(item);
            }
            Ok(res_vec)
        }
    }

    let visitor = SeqVisitor { item: PhantomData };
    deserializer.deserialize_seq(visitor)
}
