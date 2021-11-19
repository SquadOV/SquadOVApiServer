use serde_repr::{Serialize_repr, Deserialize_repr};
use num_enum::TryFromPrimitive;
use serde::{Serialize, Deserialize};

#[derive(Copy, Clone, Serialize_repr, Deserialize_repr, Debug, TryFromPrimitive, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SquadOvGames {
    AimLab,
    Hearthstone,
    LeagueOfLegends,
    TeamfightTactics,
    Valorant,
    WorldOfWarcraft,
    Csgo,
    Unknown,
}

#[derive(Copy, Clone, Serialize_repr, Deserialize_repr, Debug, TryFromPrimitive, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SquadOvWowRelease {
    Retail,
    Vanilla,
    Tbc
}

pub fn wow_release_to_db_build_expression(r: SquadOvWowRelease) -> &'static str {
    match r {
        SquadOvWowRelease::Retail => "9.%",
        SquadOvWowRelease::Vanilla => "1.%",
        SquadOvWowRelease::Tbc => "2.%",
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct FullSupportedGame {
    pub game: SquadOvGames,
    pub wow: Option<SquadOvWowRelease>,
}