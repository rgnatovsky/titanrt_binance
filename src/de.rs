// de.rs
#![allow(dead_code)]

use core::fmt;
use serde::de::{self, Visitor};
use serde::Deserializer;

#[inline]
pub fn de_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
   where
      D: Deserializer<'de>,
{
   deserializer.deserialize_any(F64Visitor)
}

struct F64Visitor;

impl<'de> Visitor<'de> for F64Visitor {
   type Value = f64;

   fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
      f.write_str("f64 as number or string")
   }

   #[inline]
   fn visit_f64<E>(self, v: f64) -> Result<f64, E>
      where
         E: de::Error,
   {
      ok_finite(v)
   }

   #[inline]
   fn visit_i64<E>(self, v: i64) -> Result<f64, E>
      where
         E: de::Error,
   {
      ok_finite(v as f64)
   }

   #[inline]
   fn visit_u64<E>(self, v: u64) -> Result<f64, E>
      where
         E: de::Error,
   {
      ok_finite(v as f64)
   }

   #[inline]
   fn visit_str<E>(self, v: &str) -> Result<f64, E>
      where
         E: de::Error,
   {
      parse_str::<E>(v)
   }

   #[inline]
   fn visit_borrowed_str<E>(self, v: &'de str) -> Result<f64, E>
      where
         E: de::Error,
   {
      parse_str::<E>(v)
   }

   #[inline]
   fn visit_string<E>(self, v: String) -> Result<f64, E>
      where
         E: de::Error,
   {
      // без аллокаций не обойтись, если приходит String; парсим напрямую
      parse_str::<E>(&v)
   }
}

#[inline]
fn ok_finite<E: de::Error>(v: f64) -> Result<f64, E> {
   if v.is_finite() {
      Ok(v)
   } else {
      Err(de::Error::custom("non-finite float"))
   }
}

#[inline]
fn parse_str<E: de::Error>(s: &str) -> Result<f64, E> {
   let s = trim_ws(s);
   if s.is_empty() {
      return Err(de::Error::invalid_value(de::Unexpected::Str(s), &"non-empty float string"));
   }

   // Быстрый путь: fast_float -> lexical-core -> std
   #[cfg(feature = "fast-float")]
   {
      match fast_float::parse::<f64, _>(s) {
         Ok(v) => return ok_finite::<E>(v),
         Err(_) => return Err(de::Error::invalid_value(de::Unexpected::Str(s), &"valid float")),
      }
   }

   #[cfg(all(not(feature = "fast-float"), feature = "lexical"))]
   {
      match lexical_core::parse::<f64>(s.as_bytes()) {
         Ok(v) => return ok_finite::<E>(v),
         Err(_) => return Err(de::Error::invalid_value(de::Unexpected::Str(s), &"valid float")),
      }
   }

   #[cfg(all(not(feature = "fast-float"), not(feature = "lexical")))]
   {
      match s.parse::<f64>() {
         Ok(v) => return ok_finite::<E>(v),
         Err(_) => return Err(de::Error::invalid_value(de::Unexpected::Str(s), &"valid float")),
      }
   }
}

#[inline]
fn trim_ws(s: &str) -> &str {
   // JSON обычно без пробелов внутри строк, но защитимся.
   let bytes = s.as_bytes();
   let mut start = 0;
   let mut end = bytes.len();
   while start < end && bytes[start].is_ascii_whitespace() { start += 1; }
   while start < end && bytes[end - 1].is_ascii_whitespace() { end -= 1; }
   &s[start..end]
}

// de.rs

// ... здесь твой код de_f64 ...

#[cfg(test)]
mod tests {
   use super::de_f64;
   use serde::Deserialize;

   #[derive(Debug, Deserialize)]
   struct HasF64 {
      #[serde(deserialize_with = "super::de_f64")]
      x: f64,
   }

   #[derive(Debug, Deserialize)]
   struct BookTicker<'a> {
      u: u64,
      #[serde(borrow)]
      s: &'a str,
      #[serde(deserialize_with = "super::de_f64")]
      b: f64,
      #[serde(deserialize_with = "super::de_f64")]
      B: f64,
      #[serde(deserialize_with = "super::de_f64")]
      a: f64,
      #[serde(deserialize_with = "super::de_f64")]
      A: f64,
   }

   #[test]
   fn number_ok() {
      let v: HasF64 = serde_json::from_str(r#"{ "x": 1.2345 }"#).unwrap();
      assert!((v.x - 1.2345).abs() < 1e-12);
   }

   #[test]
   fn integer_ok() {
      let v: HasF64 = serde_json::from_str(r#"{ "x": 42 }"#).unwrap();
      assert_eq!(v.x, 42.0);
   }

   #[test]
   fn string_ok() {
      let v: HasF64 = serde_json::from_str(r#"{ "x": "1.2345" }"#).unwrap();
      assert!((v.x - 1.2345).abs() < 1e-12);
   }

   #[test]
   fn string_ws_ok() {
      let v: HasF64 = serde_json::from_str(r#"{ "x": "   3.14  " }"#).unwrap();
      assert!((v.x - 3.14).abs() < 1e-12);
   }

   #[test]
   fn scientific_notation_ok() {
      let v: HasF64 = serde_json::from_str(r#"{ "x": "6.022e23" }"#).unwrap();
      assert!((v.x / 6.022e23 - 1.0).abs() < 1e-12);
   }

   #[test]
   fn nan_rejected() {
      // "NaN" внутри строки — парсер вернёт NaN или ошибку,
      // но наш de_f64 отфильтрует не-конечные значения → Err.
      let res: Result<HasF64, _> = serde_json::from_str(r#"{ "x": "NaN" }"#);
      assert!(res.is_err());
   }

   #[test]
   fn inf_rejected() {
      let res: Result<HasF64, _> = serde_json::from_str(r#"{ "x": "inf" }"#);
      assert!(res.is_err());

      let res: Result<HasF64, _> = serde_json::from_str(r#"{ "x": "-inf" }"#);
      assert!(res.is_err());
   }

   #[test]
   fn empty_string_rejected() {
      let res: Result<HasF64, _> = serde_json::from_str(r#"{ "x": "" }"#);
      assert!(res.is_err());
   }

   #[test]
   fn book_ticker_mixed_ok() {
      let json = r#"{
            "u": 123456789,
            "s": "BTCUSDT",
            "b": "65000.1",
            "B": "0.123",
            "a": 65000.2,
            "A": 0.456
        }"#;

      let bt: BookTicker = serde_json::from_str(json).unwrap();
      assert_eq!(bt.u, 123456789);
      assert_eq!(bt.s, "BTCUSDT");
      assert!((bt.b - 65000.1).abs() < 1e-9);
      assert!((bt.B - 0.123).abs() < 1e-12);
      assert!((bt.a - 65000.2).abs() < 1e-9);
      assert!((bt.A - 0.456).abs() < 1e-12);
   }

   #[test]
   fn rejects_bool() {
      let res: Result<HasF64, _> = serde_json::from_str(r#"{ "x": true }"#);
      assert!(res.is_err());
   }
}
