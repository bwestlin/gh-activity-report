use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, TimeZone, Utc};
use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Period {
    Yesterday,
    Today,
    LastWeek,
    ThisWeek,
    #[value(name = "last-7-days")]
    Last7Days,
}

#[derive(Clone, Debug, Serialize)]
pub struct Timeframe {
    pub label: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub start_date: NaiveDate,
    /// Inclusive date used for human-readable output.
    pub end_date: NaiveDate,
}

impl Timeframe {
    pub fn resolve(
        period: Option<Period>,
        since: Option<NaiveDate>,
        until: Option<NaiveDate>,
    ) -> Result<Self> {
        if period.is_some() && since.is_some() {
            bail!("a period and --since cannot be used together");
        }
        if until.is_some() && since.is_none() {
            bail!("--until requires --since");
        }

        let today = Local::now().date_naive();
        let (start_date, end_exclusive, label) = if let Some(start) = since {
            let inclusive_end = until.unwrap_or(start);
            if inclusive_end < start {
                bail!("--until must be the same as or later than --since");
            }
            let end = inclusive_end
                .checked_add_days(Days::new(1))
                .ok_or_else(|| anyhow::anyhow!("date range is too large"))?;
            (start, end, "custom range".to_owned())
        } else {
            dates_for_period(period.unwrap_or(Period::Yesterday), today)?
        };

        let start = local_midnight(start_date)?.with_timezone(&Utc);
        let end = local_midnight(end_exclusive)?.with_timezone(&Utc);
        let end_date = end_exclusive
            .checked_sub_days(Days::new(1))
            .ok_or_else(|| anyhow::anyhow!("invalid end date"))?;
        Ok(Self {
            label,
            start,
            end,
            start_date,
            end_date,
        })
    }

    pub fn contains(&self, timestamp: DateTime<Utc>) -> bool {
        timestamp >= self.start && timestamp < self.end
    }

    pub fn display_dates(&self) -> String {
        if self.start_date == self.end_date {
            self.start_date.format("%Y-%m-%d").to_string()
        } else {
            format!(
                "{} to {}",
                self.start_date.format("%Y-%m-%d"),
                self.end_date.format("%Y-%m-%d")
            )
        }
    }
}

fn dates_for_period(period: Period, today: NaiveDate) -> Result<(NaiveDate, NaiveDate, String)> {
    let one_day = Days::new(1);
    let yesterday = today
        .checked_sub_days(one_day)
        .ok_or_else(|| anyhow::anyhow!("date underflow"))?;
    let monday = today
        .checked_sub_days(Days::new(today.weekday().num_days_from_monday().into()))
        .ok_or_else(|| anyhow::anyhow!("date underflow"))?;

    let value = match period {
        Period::Yesterday => (yesterday, today, "yesterday"),
        Period::Today => (
            today,
            today
                .checked_add_days(one_day)
                .ok_or_else(|| anyhow::anyhow!("date overflow"))?,
            "today",
        ),
        Period::ThisWeek => (
            monday,
            today
                .checked_add_days(one_day)
                .ok_or_else(|| anyhow::anyhow!("date overflow"))?,
            "this week",
        ),
        Period::LastWeek => {
            let start = monday
                .checked_sub_days(Days::new(7))
                .ok_or_else(|| anyhow::anyhow!("date underflow"))?;
            (start, monday, "last week")
        }
        Period::Last7Days => {
            let start = today
                .checked_sub_days(Days::new(7))
                .ok_or_else(|| anyhow::anyhow!("date underflow"))?;
            (start, today, "the last 7 complete days")
        }
    };
    Ok((value.0, value.1, value.2.to_owned()))
}

fn local_midnight(date: NaiveDate) -> Result<DateTime<Local>> {
    let value = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    match Local.from_local_datetime(&value) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(first, _) => Ok(first),
        LocalResult::None => bail!("local time {date} 00:00 does not exist"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    #[test]
    fn last_week_is_previous_monday_through_sunday() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap(); // Tuesday
        let (start, end, _) = dates_for_period(Period::LastWeek, today).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 10, 19).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(start.weekday(), Weekday::Mon);
    }

    #[test]
    fn last_seven_days_excludes_today() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let (start, end, _) = dates_for_period(Period::Last7Days, today).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
        assert_eq!(end, today);
    }
}
