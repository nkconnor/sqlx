use std::convert::TryInto;
use std::mem;

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use crate::decode::{Decode, DecodeError};
use crate::encode::Encode;
use crate::postgres::protocol::TypeId;
use crate::postgres::types::PgTypeInfo;
use crate::postgres::Postgres;
use crate::types::HasSqlType;

impl HasSqlType<NaiveTime> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::TIME, "time")
    }
}

impl HasSqlType<NaiveDate> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::DATE, "date")
    }
}

impl HasSqlType<NaiveDateTime> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::TIMESTAMP, "timestamp")
    }
}

impl<Tz> HasSqlType<DateTime<Tz>> for Postgres
where
    Tz: TimeZone,
{
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::TIMESTAMPTZ, "timestamptz")
    }
}

impl HasSqlType<[NaiveTime]> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::ARRAY_TIME, "time[]")
    }
}

impl HasSqlType<[NaiveDate]> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::ARRAY_DATE, "date[]")
    }
}

impl HasSqlType<[NaiveDateTime]> for Postgres {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::ARRAY_TIMESTAMP, "timestamp[]")
    }
}

impl<Tz> HasSqlType<[DateTime<Tz>]> for Postgres
where
    Tz: TimeZone,
{
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::new(TypeId::ARRAY_TIMESTAMPTZ, "timestamp[]")
    }
}

impl Decode<Postgres> for NaiveTime {
    fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        let micros: i64 = Decode::<Postgres>::decode(raw)?;

        Ok(NaiveTime::from_hms(0, 0, 0) + Duration::microseconds(micros))
    }
}

impl Encode<Postgres> for NaiveTime {
    fn encode(&self, buf: &mut Vec<u8>) {
        let micros = (*self - NaiveTime::from_hms(0, 0, 0))
            .num_microseconds()
            .expect("shouldn't overflow");

        Encode::<Postgres>::encode(&micros, buf);
    }

    fn size_hint(&self) -> usize {
        mem::size_of::<i64>()
    }
}

impl Decode<Postgres> for NaiveDate {
    fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        let days: i32 = Decode::<Postgres>::decode(raw)?;

        Ok(NaiveDate::from_ymd(2000, 1, 1) + Duration::days(days as i64))
    }
}

impl Encode<Postgres> for NaiveDate {
    fn encode(&self, buf: &mut Vec<u8>) {
        let days: i32 = self
            .signed_duration_since(NaiveDate::from_ymd(2000, 1, 1))
            .num_days()
            .try_into()
            // TODO: How does Diesel handle this?
            .unwrap_or_else(|_| panic!("NaiveDate out of range for Postgres: {:?}", self));

        Encode::<Postgres>::encode(&days, buf)
    }

    fn size_hint(&self) -> usize {
        mem::size_of::<i32>()
    }
}

impl Decode<Postgres> for NaiveDateTime {
    fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        let micros: i64 = Decode::<Postgres>::decode(raw)?;

        postgres_epoch()
            .naive_utc()
            .checked_add_signed(Duration::microseconds(micros))
            .ok_or_else(|| {
                DecodeError::Message(Box::new(format!(
                    "Postgres timestamp out of range for NaiveDateTime: {:?}",
                    micros
                )))
            })
    }
}

impl Encode<Postgres> for NaiveDateTime {
    fn encode(&self, buf: &mut Vec<u8>) {
        let micros = self
            .signed_duration_since(postgres_epoch().naive_utc())
            .num_microseconds()
            .unwrap_or_else(|| panic!("NaiveDateTime out of range for Postgres: {:?}", self));

        Encode::<Postgres>::encode(&micros, buf);
    }

    fn size_hint(&self) -> usize {
        mem::size_of::<i64>()
    }
}

impl Decode<Postgres> for DateTime<Utc> {
    fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        let date_time = Decode::<Postgres>::decode(raw)?;
        Ok(DateTime::from_utc(date_time, Utc))
    }
}

impl Decode<Postgres> for DateTime<Local> {
    fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        let date_time = Decode::<Postgres>::decode(raw)?;
        Ok(Local.from_utc_datetime(&date_time))
    }
}

impl<Tz: TimeZone> Encode<Postgres> for DateTime<Tz>
where
    Tz::Offset: Copy,
{
    fn encode(&self, buf: &mut Vec<u8>) {
        Encode::<Postgres>::encode(&self.naive_utc(), buf);
    }

    fn size_hint(&self) -> usize {
        mem::size_of::<i64>()
    }
}

fn postgres_epoch() -> DateTime<Utc> {
    Utc.ymd(2000, 1, 1).and_hms(0, 0, 0)
}

#[test]
fn test_encode_datetime() {
    let mut buf = Vec::new();

    let date = postgres_epoch();
    Encode::<Postgres>::encode(&date, &mut buf);
    assert_eq!(buf, [0; 8]);
    buf.clear();

    // one hour past epoch
    let date2 = postgres_epoch() + Duration::hours(1);
    Encode::<Postgres>::encode(&date2, &mut buf);
    assert_eq!(buf, 3_600_000_000i64.to_be_bytes());
    buf.clear();

    // some random date
    let date3: NaiveDateTime = "2019-12-11T11:01:05".parse().unwrap();
    let expected = dbg!((date3 - postgres_epoch().naive_utc())
        .num_microseconds()
        .unwrap());
    Encode::<Postgres>::encode(&date3, &mut buf);
    assert_eq!(buf, expected.to_be_bytes());
    buf.clear();
}

#[test]
fn test_decode_datetime() {
    let buf = [0u8; 8];
    let date: NaiveDateTime = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2000-01-01 00:00:00");

    let buf = 3_600_000_000i64.to_be_bytes();
    let date: NaiveDateTime = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2000-01-01 01:00:00");

    let buf = 629_377_265_000_000i64.to_be_bytes();
    let date: NaiveDateTime = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2019-12-11 11:01:05");
}

#[test]
fn test_encode_date() {
    let mut buf = Vec::new();

    let date = NaiveDate::from_ymd(2000, 1, 1);
    Encode::<Postgres>::encode(&date, &mut buf);
    assert_eq!(buf, [0; 4]);
    buf.clear();

    let date2 = NaiveDate::from_ymd(2001, 1, 1);
    Encode::<Postgres>::encode(&date2, &mut buf);
    // 2000 was a leap year
    assert_eq!(buf, 366i32.to_be_bytes());
    buf.clear();

    let date3 = NaiveDate::from_ymd(2019, 12, 11);
    Encode::<Postgres>::encode(&date3, &mut buf);
    assert_eq!(buf, 7284i32.to_be_bytes());
    buf.clear();
}

#[test]
fn test_decode_date() {
    let buf = [0; 4];
    let date: NaiveDate = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2000-01-01");

    let buf = 366i32.to_be_bytes();
    let date: NaiveDate = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2001-01-01");

    let buf = 7284i32.to_be_bytes();
    let date: NaiveDate = Decode::<Postgres>::decode(&buf).unwrap();
    assert_eq!(date.to_string(), "2019-12-11");
}
