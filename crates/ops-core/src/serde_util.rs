//! PATCH semantiği için `Option<Option<T>>` yardımcıları:
//! alan yoksa `None` (dokunma), `null` ise `Some(None)` (temizle),
//! değer varsa `Some(Some(v))` (güncelle).

use serde::{Deserialize, Deserializer};

pub fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize, Default)]
    struct P {
        #[serde(default, deserialize_with = "super::double_option")]
        due: Option<Option<String>>,
    }

    #[test]
    fn absent_null_and_value() {
        let absent: P = serde_json::from_str("{}").unwrap();
        assert!(absent.due.is_none());
        let null: P = serde_json::from_str(r#"{"due":null}"#).unwrap();
        assert_eq!(null.due, Some(None));
        let val: P = serde_json::from_str(r#"{"due":"x"}"#).unwrap();
        assert_eq!(val.due, Some(Some("x".into())));
    }
}
