use super::*;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[allow(unused)]
#[derive(Deserialize)]
struct MySeqStruct {
    #[serde(with = "ser_seq_b64")]
    my_vec: Vec<[u8; 16]>,
}

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

#[allow(unused)]
#[derive(Serialize, Deserialize)]
struct MySeqStrStruct {
    #[serde(with = "ser_seq_str")]
    my_vec: Vec<String>,
    #[serde(with = "ser_seq_str")]
    my_hash_set: HashSet<String>,
}
