use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CombatLogTasks {
    Ff14Reports(String),
}