use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveTime, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

use super::enums::RoutineAction;
use crate::{OpsError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// `RoutineSchedule` metni: "HH:MM" (her gün) ya da "MON HH:MM" (haftalık).
    pub schedule: String,
    pub action_type: RoutineAction,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_result: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutinePatch {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub schedule: Option<String>,
}

/// Rutin zamanlaması. Saatler makinenin yerel saat dilimindedir; ayrı bir
/// timezone ayarı yoktur (tek kullanıcı, tek makine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutineSchedule {
    /// `None` = her gün.
    pub weekday: Option<Weekday>,
    pub time: NaiveTime,
}

impl RoutineSchedule {
    /// "HH:MM" ya da "DAY HH:MM" (DAY = MON..SUN, büyük/küçük harf duyarsız).
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        let (weekday, time_part) = match parts.as_slice() {
            [t] => (None, *t),
            [day, t] => (Some(parse_weekday(day)?), *t),
            _ => {
                return Err(OpsError::Validation(
                    "zamanlama 'HH:MM' ya da 'MON HH:MM' olmalı".into(),
                ))
            }
        };
        let Some((h, m)) = time_part.split_once(':') else {
            return Err(OpsError::Validation("saat 'HH:MM' biçiminde olmalı".into()));
        };
        let hour: u32 = h.parse().map_err(|_| OpsError::Validation("geçersiz saat".into()))?;
        let minute: u32 = m.parse().map_err(|_| OpsError::Validation("geçersiz dakika".into()))?;
        let time = NaiveTime::from_hms_opt(hour, minute, 0)
            .ok_or_else(|| OpsError::Validation("geçersiz saat/dakika".into()))?;
        Ok(Self { weekday, time })
    }

    /// `now`'dan kesinlikle sonraki ilk çalışma anı (`offset` = yerel saat dilimi).
    pub fn next_after(&self, now: DateTime<Utc>, offset: FixedOffset) -> Option<DateTime<Utc>> {
        let local_now = now.with_timezone(&offset);
        (0..8)
            .map(|add| local_now.date_naive() + Duration::days(add))
            .filter(|date| self.weekday.is_none_or(|wd| date.weekday() == wd))
            .filter_map(|date| offset.from_local_datetime(&date.and_time(self.time)).single())
            .map(|local| local.with_timezone(&Utc))
            .find(|candidate| *candidate > now)
    }
}

fn parse_weekday(code: &str) -> Result<Weekday> {
    match code.to_ascii_uppercase().as_str() {
        "MON" => Ok(Weekday::Mon),
        "TUE" => Ok(Weekday::Tue),
        "WED" => Ok(Weekday::Wed),
        "THU" => Ok(Weekday::Thu),
        "FRI" => Ok(Weekday::Fri),
        "SAT" => Ok(Weekday::Sat),
        "SUN" => Ok(Weekday::Sun),
        _ => Err(OpsError::Validation(format!("geçersiz gün: {code}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        crate::time::from_db(s).unwrap()
    }

    #[test]
    fn parse_accepts_daily_and_weekly_forms() {
        assert!(RoutineSchedule::parse("09:00").is_ok());
        assert_eq!(RoutineSchedule::parse("MON 09:30").unwrap().weekday, Some(Weekday::Mon));
        assert_eq!(RoutineSchedule::parse("sun 21:00").unwrap().weekday, Some(Weekday::Sun));
        assert!(RoutineSchedule::parse("25:00").is_err());
        assert!(RoutineSchedule::parse("09:60").is_err());
        assert!(RoutineSchedule::parse("FOO 09:00").is_err());
        assert!(RoutineSchedule::parse("dokuzda").is_err());
        assert!(RoutineSchedule::parse("MON 09:00 extra").is_err());
    }

    #[test]
    fn next_after_rolls_to_next_day_and_next_weekday() {
        let plus3 = FixedOffset::east_opt(3 * 3600).unwrap();
        // 2026-08-23 Pazar, yerel 10:00 (+03) = 07:00Z
        let now = utc("2026-08-23T07:00:00Z");

        let daily = RoutineSchedule::parse("09:00").unwrap();
        assert_eq!(daily.next_after(now, plus3), Some(utc("2026-08-24T06:00:00Z")));

        let later_today = RoutineSchedule::parse("10:00").unwrap();
        assert_eq!(
            later_today.next_after(now, plus3),
            Some(utc("2026-08-23T07:00:00Z").checked_add_signed(Duration::days(1)).unwrap()),
            "tam şu an olan saat bugüne değil yarına kurulur"
        );

        let evening = RoutineSchedule::parse("21:30").unwrap();
        assert_eq!(evening.next_after(now, plus3), Some(utc("2026-08-23T18:30:00Z")));

        let monday = RoutineSchedule::parse("MON 09:30").unwrap();
        assert_eq!(monday.next_after(now, plus3), Some(utc("2026-08-24T06:30:00Z")));

        let sunday = RoutineSchedule::parse("SUN 09:00").unwrap();
        assert_eq!(sunday.next_after(now, plus3), Some(utc("2026-08-30T06:00:00Z")));
    }
}
