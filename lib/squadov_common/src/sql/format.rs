use chrono::{DateTime, SecondsFormat};

pub fn sql_format_bool(v: bool) -> &'static str {
    if v {
        "TRUE"
    } else {
        "FALSE"
    }
}

pub fn sql_format_option_bool(v: Option<bool>) -> &'static str {
    if let Some(b) = v {
        sql_format_bool(b)
    } else {
        "NULL"
    }
}

pub fn sql_format_string(v: &str) -> String {
    format!("'{}'", v.replace("'", "''"))
}

pub fn sql_format_option_string<T>(v: &Option<T>) -> String
where T: std::fmt::Display
{
    match v {
        Some(x) => sql_format_string(&format!("{}", x)),
        None => String::from("NULL")
    }
}

pub fn sql_format_option_value<T>(v: &Option<T>) -> String
where T: std::fmt::Display
{
    match v {
        Some(x) => format!("{}", x),
        None => String::from("NULL")
    }
}

pub fn sql_format_time<T>(v: &DateTime<T>) -> String 
where T: chrono::TimeZone,
      <T as chrono::TimeZone>::Offset: std::fmt::Display
{
    format!("'{}'", v.to_rfc3339_opts(SecondsFormat::Micros, true))
}

pub fn sql_format_option_some_time<T>(v: Option<&DateTime<T>>) -> String
where T: chrono::TimeZone,
      <T as chrono::TimeZone>::Offset: std::fmt::Display
{
    match v {
        Some(x) => sql_format_time(x),
        None => String::from("NULL")
    }
}

pub fn sql_format_json<T>(v: &T) -> Result<String, crate::SquadOvError> 
where T: serde::Serialize
{
    Ok(format!(
        "'{}'", 
        serde_json::to_string(v)?
            .replace("'", "''")
    ))
}

pub fn sql_format_option_json<T>(v: &Option<T>) -> Result<String, crate::SquadOvError> 
where T: serde::Serialize
{
    Ok(
        match v {
            Some(x) => sql_format_json(x)?,
            None => String::from("NULL")
        }
    )
}

pub fn sql_format_varchar_array(v: &[String]) -> String {
    format!(
        "ARRAY [
            {}
        ]::VARCHAR[]",
        v.iter().map(|x| {
            format!("'{}'", x)
        }).collect::<Vec<String>>().join(",")
    )
}

pub fn sql_format_integer_array(v: &[i32]) -> String {
    format!(
        "ARRAY [
            {}
        ]::INTEGER[]",
        v.iter().map(|x| {
            format!("{}", x)
        }).collect::<Vec<String>>().join(",")
    )
}

pub fn sql_format_bigint_array(v: &[i64]) -> String {
    format!(
        "ARRAY [
            {}
        ]::BIGINT[]",
        v.iter().map(|x| {
            format!("{}", x)
        }).collect::<Vec<String>>().join(",")
    )
}