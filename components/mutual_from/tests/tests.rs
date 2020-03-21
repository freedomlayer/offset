use offset_mutual_from::mutual_from;

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
fn test_mutual_from_named_struct() {
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

#[derive(PartialEq, Eq, Clone, Debug)]
struct MyStruct1(u32, u32);

#[mutual_from(MyStruct1)]
#[derive(PartialEq, Eq, Clone, Debug)]
struct MyStruct2(u32, u32);

#[test]
fn test_mutual_from_unnamed_struct() {
    let my_struct1 = MyStruct1(1u32, 2u32);
    let my_struct2 = MyStruct2::from(my_struct1.clone());
    let my_struct1_new = MyStruct1::from(my_struct2.clone());
    let my_struct2_new = MyStruct2::from(my_struct1.clone());

    assert_eq!(my_struct1, my_struct1_new);
    assert_eq!(my_struct2, my_struct2_new);
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct World1;

#[mutual_from(World1)]
#[derive(PartialEq, Eq, Clone, Debug)]
struct World2;

#[test]
fn test_mutual_from_unit_struct() {
    let world1 = World1;
    let world2 = World2::from(world1.clone());
    let world1_new = World1::from(world2.clone());
    let world2_new = World2::from(world1_new.clone());

    assert_eq!(world1, world1_new);
    assert_eq!(world2, world2_new);
}
