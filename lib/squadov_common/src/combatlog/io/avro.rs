use crate::{
    SquadOvError,
    combatlog::{
        io::CombatLogDiskIO,
    },
};
use avro_rs::{
    Writer,
    Codec,
    Schema,
};
use serde::Serialize;

pub struct CombatLogAvroFileIO<'a> {
    writer: Writer<'a, std::fs::File>,
}

impl<'a> CombatLogDiskIO for CombatLogAvroFileIO<'a> {
    fn handle<T>(&mut self, data: T) -> Result<(), SquadOvError>
    where
        T: Serialize
    {
        self.writer.append_ser(data)?;
        Ok(())
    }

    fn get_underlying_file(self) -> Result<tokio::fs::File, SquadOvError> {
        Ok(
            tokio::fs::File::from_std(
                self.writer.into_inner()?
            )
        )
    }
}

impl<'a> CombatLogAvroFileIO<'a> {
    pub fn new(dir: &str, schema: &'a Schema) -> Result<Self, SquadOvError> {
        let file = tempfile::tempfile_in(dir)?;
        Ok(Self{
            writer: Writer::with_codec(schema, file, Codec::Snappy),
        })
    }
}