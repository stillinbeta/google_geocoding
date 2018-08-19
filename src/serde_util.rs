use serde::ser::{self, Serialize, Serializer, SerializeStructVariant, SerializeTupleVariant, Impossible};
use std;

// Many thanks to dtolnay
pub fn variant_name<T: Serialize>(t: &T) -> &'static str {
    #[derive(Debug)]
    struct NotEnum;
    type Result<T> = std::result::Result<T, NotEnum>;
    impl std::error::Error for NotEnum {
        fn description(&self) -> &str { "not struct" }
    }
    impl std::fmt::Display for NotEnum {
        fn fmt(&self, _f: &mut std::fmt::Formatter) -> std::fmt::Result { unimplemented!() }
    }
    impl ser::Error for NotEnum {
        fn custom<T: std::fmt::Display>(_msg: T) -> Self { NotEnum }
    }

    struct VariantName;
    impl Serializer for VariantName {
        type Ok = &'static str;
        type Error = NotEnum;
        type SerializeSeq = Impossible<Self::Ok, Self::Error>;
        type SerializeTuple = Impossible<Self::Ok, Self::Error>;
        type SerializeTupleStruct = Impossible<Self::Ok, Self::Error>;
        type SerializeTupleVariant = Enum;
        type SerializeMap = Impossible<Self::Ok, Self::Error>;
        type SerializeStruct = Impossible<Self::Ok, Self::Error>;
        type SerializeStructVariant = Enum;
        fn serialize_bool(self, _v: bool) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_i8(self, _v: i8) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_i16(self, _v: i16) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_i32(self, _v: i32) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_i64(self, _v: i64) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_u8(self, _v: u8) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_u16(self, _v: u16) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_u32(self, _v: u32) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_u64(self, _v: u64) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_f32(self, _v: f32) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_f64(self, _v: f64) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_char(self, _v: char) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_str(self, _v: &str) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_bytes(self, _v: &[u8]) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_none(self) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_unit(self) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str) -> Result<Self::Ok> { Ok(variant) }
        fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _name: &'static str, _value: &T) -> Result<Self::Ok> { Err(NotEnum) }
        fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _name: &'static str, _variant_index: u32, variant: &'static str, _value: &T) -> Result<Self::Ok> { Ok(variant) }
        fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> { Err(NotEnum) }
        fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> { Err(NotEnum) }
        fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct> { Err(NotEnum) }
        fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize) -> Result<Self::SerializeTupleVariant> { Ok(Enum(variant)) }
        fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> { Err(NotEnum) }
        fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> { Err(NotEnum) }
        fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant> { Ok(Enum(variant)) }
    }

    struct Enum(&'static str);
    impl SerializeStructVariant for Enum {
        type Ok = &'static str;
        type Error = NotEnum;
        fn serialize_field<T: ?Sized + Serialize>(&mut self, _key: &'static str, _value: &T) -> Result<()> { Ok(()) }
        fn end(self) -> Result<Self::Ok> {
            Ok(self.0)
        }
    }
    impl SerializeTupleVariant for Enum {
        type Ok = &'static str;
        type Error = NotEnum;
        fn serialize_field<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
        fn end(self) -> Result<Self::Ok> {
            Ok(self.0)
        }
    }

    t.serialize(VariantName).unwrap()
}
