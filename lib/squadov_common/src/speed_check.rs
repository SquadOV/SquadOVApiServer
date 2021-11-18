pub mod manager;
// pub mod db;

use serde::{Serialize,Deserialize};
use std::str;
use std::clone::Clone;

#[derive(Serialize,Deserialize, Clone)]
pub struct SpeedCheckDestination {
    pub url: String,
    pub bucket: String,
    pub session: String,
    pub loc: manager::SpeedCheckManagerType,
}