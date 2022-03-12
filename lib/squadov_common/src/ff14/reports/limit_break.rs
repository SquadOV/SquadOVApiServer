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
        agg::{
            InputAggregatorPacket,
            CombatLogAggregator,
            sliding_window::{
                CombatLogSlidingWindowAggregator,
                SlidingWindowFunction,
            },
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
use std::time::Duration;

pub struct Ff14LimitBreakReportGenerator<'a> {
    writer: Option<CombatLogAvroFileIO<'a>>,
    agg: CombatLogSlidingWindowAggregator<i64>,
}

#[derive(Serialize)]
pub struct Ff14LimitBreakEvent {
    tm: DateTime<Utc>,
    value: i64,
}

impl From<Ff14LimitBreakEvent> for InputAggregatorPacket<i64> {
    fn from(v: Ff14LimitBreakEvent) -> Self {
        Self {
            tm: v.tm,
            data: v.value,
        }
    }
}

const LIMIT_BREAK_REPORT_SCHEMA_RAW: &'static str = r#"
    {
        "type": "record",
        "name": "ff14_death_event",
        "fields": [
            {"name": "start", "type": "timestamp-millis"},
            {"name": "end", "type": "timestamp-millis"},
            {"name": "value", "type": "long"}
        ]
    }
"#;

lazy_static! {
    static ref LIMIT_BREAK_REPORT_SCHEMA: Schema = Schema::parse_str(LIMIT_BREAK_REPORT_SCHEMA_RAW).unwrap();
}

impl<'a> CombatLogReportHandler for Ff14LimitBreakReportGenerator<'a> {
    type Data = Ff14CombatLogPacket;
    fn handle(&mut self, data: &Self::Data) -> Result<(), SquadOvError> {
        match &data.data {
            Ff14PacketData::Parsed{
                inner: Ff14CombatLogEvent::LimitBreak{
                    value,
                    ..
                }
            } => {
                self.write_data(Some(Ff14LimitBreakEvent{
                    tm: data.time.clone(),
                    value: *value,
                }))?;
            },
            _ => (),
        }
        Ok(())
    }
}

impl<'a> CombatLogReportIO for Ff14LimitBreakReportGenerator<'a> {
    fn finalize(&mut self) -> Result<(), SquadOvError> {
        self.write_data(None)
    }

    fn initialize_work_dir(&mut self, dir: &str) -> Result<(), SquadOvError> {
        self.writer = Some(
            CombatLogAvroFileIO::new(dir, &LIMIT_BREAK_REPORT_SCHEMA)?
        );
        Ok(())
    }

    fn get_reports(&mut self) -> Result<Vec<RawStaticCombatLogReport>, SquadOvError> {
        let writer = self.writer.take();
        Ok(
            if let Some(w) = writer {
                vec![
                    RawStaticCombatLogReport{
                        key_name: String::from("limit_break.avro"),
                        raw_file: w.get_underlying_file()?,
                        canonical_type: Ff14ReportTypes::LimitBreak as i32,
                    }
                ]
            } else {
                vec![]
            }
        )
    }
}

impl<'a> Ff14LimitBreakReportGenerator<'a> {
    pub fn new(start_tm: DateTime<Utc>) -> Self {
        Self {
            writer: None,
            agg: CombatLogSlidingWindowAggregator::new(
                SlidingWindowFunction::Average,
                Duration::from_secs(5),
                start_tm,
            ),
        }
    }

    fn write_data(&mut self, data: Option<Ff14LimitBreakEvent>) -> Result<(), SquadOvError> {
        let packet = if let Some(d) = data {
            self.agg.handle(d.into())?
        } else {
            self.agg.flush()?
        };

        if let Some(packet) = packet {
            if let Some(w) = self.writer.as_mut() {
                w.handle(packet)?;
            }
        }
        
        Ok(())
    }
}