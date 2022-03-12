use crate::{
    SquadOvError,
    combatlog::{
        CombatLogReportHandler,
        CombatLogReportIO,
        RawStaticCombatLogReport,
        io::{
            CombatLogDiskIO,
            avro::CombatLogAvroFileIO,
        },
    },
    ff14::{
        combatlog::{
            Ff14CombatLogPacket,
            Ff14PacketData,
            Ff14CombatLogEvent,
        },
        reports::Ff14ReportTypes,
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use avro_rs::{
    Schema,
};

#[derive(Default)]
pub struct Ff14DeathReportGenerator<'a> {
    writer: Option<CombatLogAvroFileIO<'a>>,
}

#[derive(Serialize)]
pub struct Ff14DeathReportEvent {
    tm: DateTime<Utc>,
    killer: i64,
    victim: i64,
}

const DEATH_REPORT_SCHEMA_RAW: &'static str = r#"
    {
        "type": "record",
        "name": "ff14_death_event",
        "fields": [
            {"name": "tm", "type": "timestamp-millis"},
            {"name": "killer", "type": "long"},
            {"name": "victim", "type": "long"}
        ]
    }
"#;

lazy_static! {
    static ref DEATH_REPORT_SCHEMA: Schema = Schema::parse_str(DEATH_REPORT_SCHEMA_RAW).unwrap();
}

impl<'a> CombatLogReportHandler for Ff14DeathReportGenerator<'a> {
    type Data = Ff14CombatLogPacket;
    fn handle(&mut self, data: &Self::Data) -> Result<(), SquadOvError> {
        match &data.data {
            Ff14PacketData::Parsed{
                inner: Ff14CombatLogEvent::NetworkDeath{
                    target_id,
                    source_id,
                    ..
                }
            } => {
                if let Some(w) = self.writer.as_mut() {
                    w.handle(Ff14DeathReportEvent{
                        tm: data.time.clone(),
                        killer: *source_id,
                        victim: *target_id,
                    })?;
                }
            },
            _ => (),
        }
        Ok(())
    }
}

impl<'a> CombatLogReportIO for Ff14DeathReportGenerator<'a> {
    fn finalize(&mut self) -> Result<(), SquadOvError> {
        Ok(())
    }

    fn initialize_work_dir(&mut self, dir: &str) -> Result<(), SquadOvError> {
        self.writer = Some(
            CombatLogAvroFileIO::new(dir, &DEATH_REPORT_SCHEMA)?
        );
        Ok(())
    }

    fn get_reports(&mut self) -> Result<Vec<RawStaticCombatLogReport>, SquadOvError> {
        let writer = self.writer.take();
        Ok(
            if let Some(w) = writer {
                vec![
                    RawStaticCombatLogReport{
                        key_name: String::from("deaths.avro"),
                        raw_file: w.get_underlying_file()?,
                        canonical_type: Ff14ReportTypes::Deaths as i32,
                    }
                ]
            } else {
                vec![]
            }
        )
    }
}