mod death;
mod limit_break;

use crate::{
    SquadOvError,
    combatlog::{
        CombatLogReportHandler,
        CombatLogReportIO,
        RawStaticCombatLogReport,
    },
    ff14::combatlog::Ff14CombatLogPacket,
};
use num_enum::TryFromPrimitive;
use chrono::{DateTime, Utc};

pub struct Ff14ReportsGenerator<'a> {
    start_time: DateTime<Utc>,
    death_report: Option<death::Ff14DeathReportGenerator<'a>>,
    limit_break_report: Option<limit_break::Ff14LimitBreakReportGenerator<'a>>,
    work_dir: Option<String>,
}

#[derive(Copy, Clone, Debug, TryFromPrimitive, PartialEq)]
#[repr(i32)]
pub enum Ff14ReportTypes {
    Deaths,
    LimitBreak,
}

impl<'a> CombatLogReportHandler for Ff14ReportsGenerator<'a> {
    type Data = Ff14CombatLogPacket;
    fn handle(&mut self, data: &Self::Data) -> Result<(), SquadOvError> {
        if let Some(dr) = self.death_report.as_mut() {
            dr.handle(data)?;
        }
        Ok(())
    }
}

impl<'a> CombatLogReportIO for Ff14ReportsGenerator<'a> {
    fn finalize(&mut self) -> Result<(), SquadOvError> {
        if let Some(dr) = self.death_report.as_mut() {
            dr.finalize()?;
        }
        Ok(())
    }

    fn initialize_work_dir(&mut self, dir: &str) -> Result<(), SquadOvError> {
        self.work_dir = Some(String::from(dir));

        // Create game-wide reports here. Per player reports will be created when those players are introduced.
        {
            let mut dr = death::Ff14DeathReportGenerator::default();
            dr.initialize_work_dir(dir)?;
            self.death_report = Some(dr);
        }

        {
            let mut lb = limit_break::Ff14LimitBreakReportGenerator::new(self.start_time);
            lb.initialize_work_dir(dir)?;
            self.limit_break_report = Some(lb);
        }

        Ok(())
    }

    fn get_reports(&mut self) -> Result<Vec<RawStaticCombatLogReport>, SquadOvError> {
        let mut ret: Vec<RawStaticCombatLogReport> = vec![];

        if let Some(dr) = self.death_report.as_mut() {
            ret.extend(dr.get_reports()?);
        }

        Ok(ret)
    }
}

impl<'a> Ff14ReportsGenerator<'a> {
    pub fn new(start_time: DateTime<Utc>) -> Result<Self, SquadOvError> {
        Ok(Self{
            start_time,
            death_report: None,
            limit_break_report: None,
            work_dir: None,
        })
    }
}