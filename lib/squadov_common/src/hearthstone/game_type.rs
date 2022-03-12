use derive_more::{Display};
use serde_repr::{Serialize_repr, Deserialize_repr};
use num_enum::TryFromPrimitive;

#[derive(sqlx::Type, Display, Clone, Copy, Serialize_repr, Deserialize_repr, TryFromPrimitive, PartialEq, Debug)]
#[repr(i32)]
pub enum GameType {
    Unknown = 0,
    VsAi = 1,
    VsFriend = 2,
    Tutorial = 4,
    Arena = 5,
    TestAiVsAi = 6,
    Ranked = 7,
    Casual = 8,
    TavernBrawl = 0x10,
    Tb1pVsAi = 17,
    Tb2pCoop = 18,
    FsgBrawlVsFriend = 19,
    FsgBrawl = 20,
    FsgBrawl1pVsAi = 21,
    FsgBrawl2pCoop = 22,
    Battlegrounds = 23,
    BattlegroundsFriendly = 24,
    Reserved1822 = 26,
    Reserved1823 = 27,
    PvpDrPaid = 28,
    PvpDr = 29
}

pub fn get_all_hearthstone_game_types() -> &'static [GameType] {
    lazy_static!{
        static ref ALL_TYPES: Vec<GameType> = vec![
            GameType::Unknown,
            GameType::VsAi,
            GameType::VsFriend,
            GameType::Tutorial,
            GameType::Arena,
            GameType::TestAiVsAi,
            GameType::Ranked,
            GameType::Casual,
            GameType::TavernBrawl,
            GameType::Tb1pVsAi,
            GameType::Tb2pCoop,
            GameType::FsgBrawlVsFriend,
            GameType::FsgBrawl,
            GameType::FsgBrawl1pVsAi,
            GameType::FsgBrawl2pCoop,
            GameType::Battlegrounds,
            GameType::BattlegroundsFriendly,
            GameType::Reserved1822,
            GameType::Reserved1823,
            GameType::PvpDrPaid,
            GameType::PvpDr,
        ];
    }
    &ALL_TYPES
}

impl std::str::FromStr for GameType {
    type Err = crate::SquadOvError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "GT_UNKNOWN" => GameType::Unknown,
            "GT_VS_AI" => GameType::VsAi,
            "GT_VS_FRIEND" => GameType::VsFriend,
            "GT_TUTORIAL" => GameType::Tutorial,
            "GT_ARENA" => GameType::Arena,
            "GT_TEST_AI_VS_AI" => GameType::TestAiVsAi,
            "GT_RANKED" => GameType::Ranked,
            "GT_CASUAL" => GameType::Casual,
            "GT_TAVERNBRAWL" => GameType::TavernBrawl,
            "GT_TB_1P_VS_AI" => GameType::Tb1pVsAi,
            "GT_TB_2P_COOP" => GameType::Tb2pCoop,
            "GT_FSG_BRAWL_VS_FRIEND" => GameType::FsgBrawlVsFriend,
            "GT_FSG_BRAWL" => GameType::FsgBrawl,
            "GT_FSG_BRAWL_1P_VS_AI" => GameType::FsgBrawl1pVsAi,
            "GT_FSG_BRAWL_2P_COOP" => GameType::FsgBrawl2pCoop,
            "GT_BATTLEGROUNDS" => GameType::Battlegrounds,
            "GT_BATTLEGROUNDS_FRIENDLY" => GameType::BattlegroundsFriendly,
            "GT_RESERVED_18_22" => GameType::Reserved1822,
            "GT_RESERVED_18_23" => GameType::Reserved1823,
            "GT_PVPDR_PAID" => GameType::PvpDrPaid,
            "GT_PVPDR" => GameType::PvpDr,
            _ => GameType::Unknown
        })
    }
}