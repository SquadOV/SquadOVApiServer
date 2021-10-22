use uuid::Uuid;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub struct LolMatchLink {
    pub match_uuid: Uuid,
    pub platform: String,
    pub match_id: i64
}

pub type LolMatchlistDto = Vec<String>;

#[derive(Deserialize)]
pub struct LolMatchReferenceDto {
    #[serde(rename="gameId")]
    pub game_id: i64,
    #[serde(rename="platformId")]
    pub platform_id: String
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolMatchMetadataDto {
    pub data_version: String,
    pub match_id: String,
    pub participants: Vec<String>,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolMatchInfoDto {
    #[serde(deserialize_with="crate::parse_utc_time_from_milliseconds")]
    pub game_creation: Option<DateTime<Utc>>, // timestamp when champ select ended and loading screen appears
    pub game_duration: i64, // milliseconds or seconds depending on the game...
    pub game_end_timestamp: Option<i64>,
    pub game_id: i64,
    pub game_mode: String,
    pub game_name: String,
    pub game_start_timestamp: i64,
    pub game_type: String,
    pub game_version: String,
    pub map_id: i32,
    pub participants: Vec<LolParticipantDto>,
    pub queue_id: i32,
    pub teams: Vec<LolTeamDto>,
}

#[derive(Serialize,Deserialize)]
pub struct LolMatchDto {
    pub metadata: LolMatchMetadataDto,
    pub info: LolMatchInfoDto,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolTeamDto {
    pub team_id: i32,
    pub win: bool,
    pub bans: Vec<LolBanDto>,
    pub objectives: LolObjectivesDto,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolBanDto {
    pub champion_id: i32,
    pub pick_turn: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolObjectivesDto {
    pub baron: LolSingleObjectiveDto,
    pub champion: LolSingleObjectiveDto,
    pub dragon: LolSingleObjectiveDto,
    pub inhibitor: LolSingleObjectiveDto,
    pub rift_herald: LolSingleObjectiveDto,
    pub tower: LolSingleObjectiveDto,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolSingleObjectiveDto {
    pub first: bool,
    pub kills: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all="camelCase")]
pub struct LolParticipantDto {
    #[serde(default)]
    pub assists: i32,
    #[serde(default)]
    pub baron_kills: i32,
    #[serde(default)]
    pub bounty_level: i32,
    #[serde(default)]
    pub champ_experience: i32,
    #[serde(default)]
    pub champ_level: i32,
    #[serde(default)]
    pub champion_id: i32,
    #[serde(default)]
    pub champion_name: String,
    #[serde(default)]
    pub champion_transform: i32,
    #[serde(default)]
    pub consumables_purchased: i32,
    #[serde(default)]
    pub damage_dealt_to_buildings: i32,
    #[serde(default)]
    pub damage_dealt_to_objectives: i32,
    #[serde(default)]
    pub damage_dealt_to_turrets: i32,
    #[serde(default)]
    pub damage_self_mitigated: i32,
    #[serde(default)]
    pub deaths: i32,
    #[serde(default)]
    pub detector_wards_placed: i32,
    #[serde(default)]
    pub double_kills: i32,
    #[serde(default)]
    pub dragon_kills: i32,
    #[serde(default)]
    pub first_blood_assist: bool,
    #[serde(default)]
    pub first_blood_kill: bool,
    #[serde(default)]
    pub first_tower_assist: bool,
    #[serde(default)]
    pub first_tower_kill: bool,
    #[serde(default)]
    pub game_ended_in_early_surrender: bool,
    #[serde(default)]
    pub game_ended_in_surrender: bool,
    #[serde(default)]
    pub gold_earned: i32,
    #[serde(default)]
    pub gold_spent: i32,
    #[serde(default)]
    pub individual_position: String,
    #[serde(default)]
    pub inhibitor_kills: i32,
    #[serde(default)]
    pub inhibitor_takedowns: i32,
    #[serde(default)]
    pub inhibitors_lost: i32,
    #[serde(rename="item0", default)]
    pub item0: i32,
    #[serde(rename="item1", default)]
    pub item1: i32,
    #[serde(rename="item2", default)]
    pub item2: i32,
    #[serde(rename="item3", default)]
    pub item3: i32,
    #[serde(rename="item4", default)]
    pub item4: i32,
    #[serde(rename="item5", default)]
    pub item5: i32,
    #[serde(rename="item6", default)]
    pub item6: i32,
    #[serde(default)]
    pub items_purchased: i32,
    #[serde(default)]
    pub killing_sprees: i32,
    #[serde(default)]
    pub kills: i32,
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub largest_critical_strike: i32,
    #[serde(default)]
    pub largest_killing_spree: i32,
    #[serde(default)]
    pub largest_multi_kill: i32,
    #[serde(default)]
    pub longest_time_spent_living: i32,
    #[serde(default)]
    pub magic_damage_dealt: i32,
    #[serde(default)]
    pub magic_damage_dealt_to_champions: i32,
    #[serde(default)]
    pub magic_damage_taken: i32,
    #[serde(default)]
    pub neutral_minions_killed: i32,
    #[serde(default)]
    pub nexus_kills: i32,
    #[serde(default)]
    pub nexus_takedowns: i32,
    #[serde(default)]
    pub nexus_lost: i32,
    #[serde(default)]
    pub objectives_stolen: i32,
    #[serde(default)]
    pub objectives_stolen_assists: i32,
    #[serde(default)]
    pub participant_id: i32,
    #[serde(default)]
    pub penta_kills: i32,
    pub perks: LolPerksDto,
    #[serde(default)]
    pub physical_damage_dealt: i32,
    #[serde(default)]
    pub physical_damage_dealt_to_champions: i32,
    #[serde(default)]
    pub physical_damage_taken: i32,
    #[serde(default)]
    pub profile_icon: i32,
    #[serde(default)]
    pub puuid: String,
    #[serde(default)]
    pub quadra_kills: i32,
    #[serde(default)]
    pub riot_id_name: String,
    #[serde(default)]
    pub riot_id_tagline: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub sight_wards_bought_in_game: i32,
    #[serde(rename="spell1Casts", default)]
    pub spell1_casts: i32,
    #[serde(rename="spell2Casts", default)]
    pub spell2_casts: i32,
    #[serde(rename="spell3Casts", default)]
    pub spell3_casts: i32,
    #[serde(rename="spell4Casts", default)]
    pub spell4_casts: i32,
    #[serde(rename="summoner1Casts", default)]
    pub summoner1_casts: i32,
    #[serde(rename="summoner1Id", default)]
    pub summoner1_id: i32,
    #[serde(rename="summoner2Casts", default)]
    pub summoner2_casts: i32,
    #[serde(rename="summoner2Id", default)]
    pub summoner2_id: i32,
    #[serde(default)]
    pub summoner_id: String,
    #[serde(default)]
    pub summoner_level: i32,
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub team_early_surrendered: bool,
    #[serde(default)]
    pub team_id: i32,
    #[serde(default)]
    pub team_position: String,
    #[serde(rename="timeCCingOthers", default)]
    pub time_ccing_others: i32,
    #[serde(default)]
    pub time_played: i32,
    #[serde(default)]
    pub total_damage_dealt: i32,
    #[serde(default)]
    pub total_damage_dealt_to_champions: i32,
    #[serde(default)]
    pub total_damage_shielded_on_teammates: i32,
    #[serde(default)]
    pub total_damage_taken: i32,
    #[serde(default)]
    pub total_heal: i32,
    #[serde(default)]
    pub total_heals_on_teammates: i32,
    #[serde(default)]
    pub total_minions_killed: i32,
    #[serde(rename="totalTimeCCDealt", default)]
    pub total_time_cc_dealt: i32,
    #[serde(default)]
    pub total_time_spent_dead: i32,
    #[serde(default)]
    pub total_units_healed: i32,
    #[serde(default)]
    pub triple_kills: i32,
    #[serde(default)]
    pub true_damage_dealt: i32,
    #[serde(default)]
    pub true_damage_dealt_to_champions: i32,
    #[serde(default)]
    pub true_damage_taken: i32,
    #[serde(default)]
    pub turret_kills: i32,
    #[serde(default)]
    pub turret_takedowns: i32,
    #[serde(default)]
    pub turrets_lost: i32,
    #[serde(default)]
    pub unreal_kills: i32,
    #[serde(default)]
    pub vision_score: i32,
    #[serde(default)]
    pub vision_wards_bought_in_game: i32,
    #[serde(default)]
    pub wards_killed: i32,
    #[serde(default)]
    pub wards_placed: i32,
    #[serde(default)]
    pub win: bool,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolPerksDto {
    pub stat_perks: LolPerkStatsDto,
    pub styles: Vec<LolPerkStyleDto>,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolPerkStatsDto {
    pub defense: i32,
    pub flex: i32,
    pub offense: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolPerkStyleDto {
    pub description: String,
    pub selections: Vec<LolPerkStyleSelectionDto>,
    pub style: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolPerkStyleSelectionDto {
    pub perk: i32,
    pub var1: i32,
    pub var2: i32,
    pub var3: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMatchTimelineDto {
    pub metadata: LolMatchMetadataDto,
    pub info: LolMatchTimelineInfoDto,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMatchTimelineInfoDto {
    pub frames: Vec<LolMatchFrameDto>,
    pub frame_interval: i64
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMatchFrameDto {
    pub participant_frames: HashMap<String, LolMatchParticipantFrameDto>,
    pub events: Vec<LolMatchEventDto>,
    pub timestamp: i64,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMatchParticipantFrameDto {
    pub participant_id: i32,
    pub minions_killed: i32,
    pub total_gold: i32,
    pub level: i32,
    pub xp: i32,
    pub current_gold: i32,
    pub jungle_minions_killed: i32,
    pub position: Option<LolMatchPositionDto>,
}

#[derive(Serialize,Deserialize,Default)]
pub struct LolMatchPositionDto {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize,Deserialize,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMatchEventDto {
    pub lane_type: Option<String>,
    pub skill_slot: Option<i32>,
    pub ascended_type: Option<String>,
    pub creator_id: Option<i32>,
    pub after_id: Option<i32>,
    pub event_type: Option<String>,
    #[serde(rename="type")]
    pub real_type: String,
    pub level_up_type: Option<String>,
    pub ward_type: Option<String>,
    pub participant_id: Option<i32>,
    pub tower_type: Option<String>,
    pub item_id: Option<i32>,
    pub before_id: Option<i32>,
    pub monster_type: Option<String>,
    pub monster_sub_type: Option<String>,
    pub team_id: Option<i32>,
    pub position: Option<LolMatchPositionDto>,
    pub killer_id: Option<i32>,
    pub timestamp: i64,
    pub assisting_participant_ids: Option<Vec<i32>>,
    pub building_type: Option<String>,
    pub victim_id: Option<i32>,
}

#[derive(Serialize, Clone, Debug,Default)]
#[serde(rename_all = "camelCase")]
pub struct LolMiniParticipantStats {
    pub participant_id: i32,
    pub champion_id: i32,
    pub summoner_name: Option<String>,
    pub team_id: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub total_damage_dealt_to_champions: i64,
    pub total_minions_killed: i32,
    pub wards_placed: i32,
    pub lane: String,
    pub win: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LolPlayerMatchSummary {
    pub match_uuid: Uuid,
    pub user_uuid: Uuid,
    pub game_creation: DateTime<Utc>,
    pub game_duration: i64,
    pub game_type: String,
    pub game_version: String,
    pub queue_id: i32,
    pub season_id: i32,
    pub map_id: i32,
    pub game_mode: String,
    pub current_participant_id: i32,
    pub participants: Vec<LolMiniParticipantStats>,
    pub has_vod: bool,
}

impl LolPlayerMatchSummary {
    fn did_win(&self) -> bool {
        for x in &self.participants {
            if x.participant_id == self.current_participant_id {
                return x.win;
            }
        }
        false
    }

    pub fn win_loss(&self) -> String {
        if self.did_win() {
            String::from("Win")
        } else {
            String::from("Loss")
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullLolMatch {
    #[serde(rename="match")]
    pub lol_match: LolMatchDto,
    pub timeline: LolMatchTimelineDto,
    pub user_id_to_participant_id: HashMap<i64, i32>,
    pub game_start_time: Option<DateTime<Utc>>,
}