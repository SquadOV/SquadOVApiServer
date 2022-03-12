pub mod avro;

use crate::SquadOvError;
use serde::Serialize;

pub trait CombatLogDiskIO {
    fn handle<T>(&mut self, data: T) -> Result<(), SquadOvError> where T: Serialize;
    fn get_underlying_file(self) -> Result<tokio::fs::File, SquadOvError>;
}