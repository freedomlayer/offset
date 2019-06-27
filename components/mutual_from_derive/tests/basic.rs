use offst_mutual_from_derive::mutual_from;

#[derive(PartialEq, Eq, Clone, Debug)]
struct Hello1 {
    a: u32,
    b: String,
}

#[mutual_from(Hello1)]
#[derive(PartialEq, Eq, Clone, Debug)]
struct Hello2 {
    a: u32,
    b: String,
}

#[test]
fn basic() {
    let hello1 = Hello1 {
        a: 0u32,
        b: "some_string".to_owned(),
    };
    let hello2 = Hello2::from(hello1.clone());
    let hello1_new = Hello1::from(hello2.clone());
    let hello2_new = Hello2::from(hello1_new.clone());

    assert_eq!(hello1, hello1_new);
    assert_eq!(hello2, hello2_new);
}
