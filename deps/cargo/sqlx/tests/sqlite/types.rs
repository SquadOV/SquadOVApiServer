use sqlx::sqlite::Sqlite;
use sqlx_test::test_type;

test_type!(null<Option<i32>>(Sqlite,
    "NULL" == None::<i32>
));

test_type!(bool(Sqlite, "FALSE" == false, "TRUE" == true));

test_type!(i32(Sqlite, "94101" == 94101_i32));

test_type!(i64(Sqlite, "9358295312" == 9358295312_i64));

// NOTE: This behavior can be surprising. Floating-point parameters are widening to double which can
//       result in strange rounding.
test_type!(f32(Sqlite, "3.1410000324249268" == 3.141f32 as f64 as f32));

test_type!(f64(Sqlite, "939399419.1225182" == 939399419.1225182_f64));

test_type!(str<String>(Sqlite,
    "'this is foo'" == "this is foo",
    "cast(x'7468697320006973206E756C2D636F6E7461696E696E67' as text)" == "this \0is nul-containing",
    "''" == ""
));

test_type!(bytes<Vec<u8>>(Sqlite,
    "X'DEADBEEF'"
        == vec![0xDE_u8, 0xAD, 0xBE, 0xEF],
    "X''"
        == Vec::<u8>::new(),
    "X'0000000052'"
        == vec![0_u8, 0, 0, 0, 0x52]
));

#[cfg(feature = "chrono")]
mod chrono {
    use super::*;
    use sqlx::types::chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

    test_type!(chrono_naive_date_time<NaiveDateTime>(Sqlite,
        "datetime('2019-01-02 05:10:20')" == NaiveDate::from_ymd(2019, 1, 2).and_hms(5, 10, 20)
    ));

    test_type!(chrono_date_time_utc<DateTime::<Utc>>(Sqlite,
        "datetime('1996-12-20T00:39:57+00:00')" == Utc.ymd(1996, 12, 20).and_hms(0, 39, 57)
    ));

    test_type!(chrono_date_time_fixed_offset<DateTime::<FixedOffset>>(Sqlite,
        "datetime('2016-11-08T03:50:23-05:00')" == FixedOffset::west(5 * 3600).ymd(2016, 11, 08).and_hms(3, 50, 23)
    ));
}
