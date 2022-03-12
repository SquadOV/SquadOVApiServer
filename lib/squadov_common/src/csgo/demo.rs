use nom::{
    IResult,
    number::streaming::{
        le_i32,
        le_f32,
        le_u8,
        le_u32,
        le_u64,
        be_u32,
        be_u64,
        be_i32,
    },
    bytes::streaming::{
        take,
    },
    combinator::map_res,
    multi::many_m_n,
};
use crate::proto::csgo::{
    CsvcMsgGameEvent,
    CsvcMsgCreateStringTable,
    CsvcMsgUpdateStringTable,
    CsvcMsgPacketEntities,
    csvc_msg_game_event_list,
    csvc_msg_game_event,
};
use crate::SquadOvError;
use crate::parse::bit_reader::BitReader;
use super::data_table::CsgoDemoDataTable;
use super::entity::CsgoEntityScene;
use super::math::{
    CsgoVector,
    CsgoQAngle,
    parse_csgo_vector,
    parse_csgo_qangle,
};
use super::weapon::{CsgoWeapon, csgo_string_to_weapon};
use std::collections::{HashMap, VecDeque};
use std::sync::{RwLock, Arc};
use num_enum::TryFromPrimitive;
use std::convert::TryFrom;
use serde_repr::{Serialize_repr};

const SUBSTRING_BITS: usize = 5;
const MAX_USERDATA_BITS: usize = 14;

const CSGO_PLAYER_MAX_WEAPONS: i32 = 64;
const CSGO_WEAPON_ID_MASK: i32 = 0x7FF;

#[derive(Debug)]
pub struct CsgoDemoHeader {
    pub demo_filestamp: String,
    pub demo_protocol: i32,
    pub network_protocol: i32,
    pub server_name: String,
    pub client_name: String,
    pub map_name: String,
    pub game_directory: String,
    pub playback_time: f32,
    pub playback_ticks: i32,
    pub playback_frames: i32,
    pub signon_length: i32
}

impl Default for CsgoDemoHeader {
    fn default() -> Self {
        Self {
            demo_filestamp: String::new(),
            demo_protocol: 0,
            network_protocol: 0,
            server_name: String::new(),
            client_name: String::new(),
            map_name: String::new(),
            game_directory: String::new(),
            playback_time: 0.0,
            playback_ticks: 0,
            playback_frames: 0,
            signon_length: 0,
        }
    }
}

pub fn parse_csgo_demo_header(input: &[u8]) -> IResult<&[u8], CsgoDemoHeader> {
    let (input, demo_filestamp) = map_res(take(8usize), std::str::from_utf8)(input)?;
    let (input, demo_protocol) = le_i32(input)?;
    let (input, network_protocol) = le_i32(input)?;
    let (input, server_name) = map_res(take(260usize), std::str::from_utf8)(input)?;
    let (input, client_name) = map_res(take(260usize), std::str::from_utf8)(input)?;
    let (input, map_name) = map_res(take(260usize), std::str::from_utf8)(input)?;
    let (input, game_directory) = map_res(take(260usize), std::str::from_utf8)(input)?;
    let (input, playback_time) = le_f32(input)?;
    let (input, playback_ticks) = le_i32(input)?;
    let (input, playback_frames) = le_i32(input)?;
    let (input, signon_length) = le_i32(input)?;

    Ok((input, CsgoDemoHeader{
        demo_filestamp: String::from(demo_filestamp).trim_end_matches(char::from(0)).to_string(),
        demo_protocol,
        network_protocol,
        server_name: String::from(server_name).trim_end_matches(char::from(0)).to_string(),
        client_name: String::from(client_name).trim_end_matches(char::from(0)).to_string(),
        map_name: String::from(map_name).trim_end_matches(char::from(0)).to_string(),
        game_directory: String::from(game_directory).trim_end_matches(char::from(0)).to_string(),
        playback_time: playback_time,
        playback_ticks: playback_ticks,
        playback_frames: playback_frames,
        signon_length: signon_length,
    }))
}

#[repr(u8)]
#[derive(Debug)]
pub enum CsgoDemoCmdMessage {
    // Startup message
    SignOn = 1,
    // Normal network packet
    Packet,
    // Sync client clock to demo tick
    SyncTick,
    // Console command
    ConsoleCmd,
    // User Cmd
    UserCmd,
    // Network data tables
    DataTables,
    // End of time
    Stop,
    // Blob of binary data understood by a callback function
    CustomData,
    // String tables?
    StringTables
}

#[derive(Debug)]
pub struct CsgoDemoCmdHeader {
    pub cmd: CsgoDemoCmdMessage,
    pub tick: i32,
    pub player_slot: u8,
}

pub fn parse_csgo_demo_cmd_header(input: &[u8]) -> IResult<&[u8], CsgoDemoCmdHeader> {
    let (input, cmd) = le_u8(input)?;
    let (input, tick) = le_i32(input)?;
    let (input, player_slot) = le_u8(input)?;
    Ok((input, CsgoDemoCmdHeader{
        cmd: match cmd {
            1 => CsgoDemoCmdMessage::SignOn,
            2 => CsgoDemoCmdMessage::Packet,
            3 => CsgoDemoCmdMessage::SyncTick,
            4 => CsgoDemoCmdMessage::ConsoleCmd,
            5 => CsgoDemoCmdMessage::UserCmd,
            6 => CsgoDemoCmdMessage::DataTables,
            7 => CsgoDemoCmdMessage::Stop,
            8 => CsgoDemoCmdMessage::CustomData,
            9 => CsgoDemoCmdMessage::StringTables,
            _ => CsgoDemoCmdMessage::Stop,
        },
        tick,
        player_slot,
    }))
}

#[derive(Debug)]
pub struct CsgoDemoCmdSplitInfo {
    pub flags: i32,
    pub view_origin: CsgoVector,
    pub view_angles: CsgoQAngle,
    pub local_view_angles: CsgoQAngle,
    pub view_origin_2: CsgoVector,
    pub view_angles_2: CsgoQAngle,
    pub local_view_angles_2: CsgoQAngle,
}

pub fn parse_csgo_demo_cmd_split_info(input: &[u8]) -> IResult<&[u8], CsgoDemoCmdSplitInfo> {
    let (input, flags) = le_i32(input)?;
    let (input, view_origin) = parse_csgo_vector(input)?;
    let (input, view_angles) = parse_csgo_qangle(input)?;
    let (input, local_view_angles) = parse_csgo_qangle(input)?;
    let (input, view_origin_2) = parse_csgo_vector(input)?;
    let (input, view_angles_2) = parse_csgo_qangle(input)?;
    let (input, local_view_angles_2) = parse_csgo_qangle(input)?;
    Ok((input, CsgoDemoCmdSplitInfo{
        flags,
        view_origin,
        view_angles,
        local_view_angles,
        view_origin_2,
        view_angles_2,
        local_view_angles_2,
    }))
}

#[derive(Debug)]
pub struct CsgoDemoCmdInfo {
    pub splits: Vec<CsgoDemoCmdSplitInfo>
}

pub fn parse_csgo_demo_cmd_info(input: &[u8]) -> IResult<&[u8], CsgoDemoCmdInfo> {
    let (input, splits) = many_m_n(2, 2, parse_csgo_demo_cmd_split_info)(input)?;
    Ok((input, CsgoDemoCmdInfo{
        splits,
    }))
}

#[derive(Debug)]
pub struct CsgoDemoPacketMessage {
    pub cmd: i32,
    pub payload: Vec<u8>,
}

pub fn parse_csgo_demo_packet_message(input: &[u8]) -> IResult<&[u8], CsgoDemoPacketMessage> {
    let (input, cmd) = le_i32(input)?;
    let (input, sz) = le_i32(input)?;
    let (input, payload) = take(sz as usize)(input)?;
    Ok((input, CsgoDemoPacketMessage{
        cmd,
        payload: payload.to_vec(),
    }))
}

#[derive(Debug, Clone, Copy, Serialize_repr, TryFromPrimitive)]
#[repr(i32)]
pub enum CsgoDemoBombStatus {
    Planted,
    Defused,
    Exploded,
    Unknown
}

#[derive(Debug, Clone, Copy, Serialize_repr, TryFromPrimitive)]
#[repr(i32)]
pub enum CsgoDemoBombSite {
    SiteA,
    SiteB,
    SiteUnknown,
}

#[derive(Debug, Clone, Copy, Serialize_repr, TryFromPrimitive)]
#[repr(i32)]
pub enum CsgoTeam {
    TeamCT,
    TeamT,
    TeamSpectate,
}

#[derive(Debug, Clone, Copy, TryFromPrimitive, Serialize_repr)]
#[repr(i32)]
pub enum CsgoRoundWin {
    TargetBombed = 1,
    BombDefused = 7,
    CTWin,
    TWin,
    TSurrender = 17,
    CTSurrender,
    Unknown,
}

#[derive(Debug)]
pub struct CsgoDemoBombState {
    // Bomb 'event' can either be a defusal or an explosion since those two events
    // should be mutually exclusive.
    pub bomb_state: CsgoDemoBombStatus,
    pub bomb_event_tick: Option<i32>,
    pub bomb_event_userid: Option<i32>,
    pub bomb_plant_tick: Option<i32>,
    pub bomb_plant_userid: Option<i32>,
    pub bomb_plant_site: Option<CsgoDemoBombSite>,
}

impl Default for CsgoDemoBombState {
    fn default() -> Self {
        Self {
            bomb_state: CsgoDemoBombStatus::Planted,
            bomb_event_tick: None,
            bomb_event_userid: None,
            bomb_plant_tick: None,
            bomb_plant_userid: None,
            bomb_plant_site: None,
        }
    }
}

#[derive(Debug)]
pub struct CsgoDemoKill {
    pub tick: i32,
    pub victim: i32,
    // Keeping this an option just in case the player killed themselves...
    pub killer: Option<i32>,
    pub assister: Option<i32>,
    pub flash_assist: bool,
    pub headshot: bool,
    pub smoke: bool,
    pub blind: bool,
    pub wallbang: bool,
    pub noscope: bool,
    pub weapon: CsgoWeapon,
}

#[derive(Debug, Clone, Copy, TryFromPrimitive, Serialize_repr)]
#[repr(i32)]
pub enum CsgoDemoHitGroup {
    Generic = 0,
    Head,
    Chest,
    Stomach,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Gear
}

#[derive(Debug)]
pub struct CsgoDemoDamage {
    pub tick: i32,
    pub attacker: Option<i32>,
    pub receiver: i32,
    pub remaining_health: i32,
    pub remaining_armor: i32,
    pub damage_health: i32,
    pub damage_armor: i32,
    pub weapon: CsgoWeapon,
    pub hitgroup: CsgoDemoHitGroup,
}

#[derive(Debug)]
pub struct CsgoDemoRoundPlayerInfo {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub equipment_value: i32,
    pub headshot_kills: i32,
    pub objective: i32,
    pub cash_earned: i32,
    pub utility_damage: i32,
    pub enemies_flashed: i32,
    pub damage: i32,
    pub money_saved: i32,
    pub kill_reward: i32,
    pub weapons: Vec<CsgoWeapon>,
    pub armor: i32,
    pub has_defuse: bool,
    pub has_helmet: bool,
    pub money: i32,
    pub team: CsgoTeam,
}

impl Default for CsgoDemoRoundPlayerInfo {
    fn default() -> Self {
        Self {
            kills: 0,
            deaths: 0,
            assists: 0,
            equipment_value: 0,
            headshot_kills: 0,
            objective: 0,
            cash_earned: 0,
            utility_damage: 0,
            enemies_flashed: 0,
            damage: 0,
            money_saved: 0,
            kill_reward: 0,
            weapons: vec![],
            armor: 0,
            has_defuse: false,
            has_helmet: false,
            money: 0,
            team: CsgoTeam::TeamSpectate,
        }
    }
}

#[derive(Debug)]
pub struct CsgoDemoRound {
    pub round_num: usize,
    // Time bookmarks.
    pub round_start_tick: i32,
    // All these other times are filled in later (after round start).
    // I also don't want to shoe-horn us into only supporting the bomb defusal game mode
    // so freeze end (i.e buy time end) could just not be a thing in those modes.
    pub freeze_end_tick: Option<i32>,
    pub round_end_tick: Option<i32>,
    pub bomb_state: Option<CsgoDemoBombState>,
    // This is the team winner.
    pub round_winner: Option<CsgoTeam>,
    pub round_win_reason: Option<CsgoRoundWin>,
    // The user who was the "MVP" of this round.
    pub round_mvp: Option<i32>,
    // Kills and damage events that happened during this round.
    pub kills: Vec<CsgoDemoKill>,
    pub damage: Vec<CsgoDemoDamage>,
    // Players in this round and their econ/weapons.
    pub players: HashMap<i32, CsgoDemoRoundPlayerInfo>,
    pub round_frozen: bool,
}

impl Default for CsgoDemoRound {
    fn default() -> Self {
        Self {
            round_num: 0,
            round_start_tick: 0,
            freeze_end_tick: None,
            round_end_tick: None,
            bomb_state: None,
            round_winner: None,
            round_win_reason: None,
            round_mvp: None,
            kills: vec![],
            damage: vec![],
            players: HashMap::new(),
            round_frozen: false,
        }
    }
}

impl CsgoDemoRound {
    
    fn plant_bomb(&mut self, tick: i32, site: CsgoDemoBombSite, player: i32) {
        let mut new_state = CsgoDemoBombState::default();
        new_state.bomb_plant_tick = Some(tick);
        new_state.bomb_plant_userid = Some(player);
        new_state.bomb_plant_site = Some(site);
        self.bomb_state = Some(new_state);
    }

    fn defuse_bomb(&mut self, tick: i32, player: i32) {
        if let Some(state) = self.bomb_state.as_mut() {
            state.bomb_event_tick = Some(tick);
            state.bomb_event_userid = Some(player);
            state.bomb_state = CsgoDemoBombStatus::Defused;
        }
    }

    fn explode_bomb(&mut self, tick: i32) {
        if let Some(state) = self.bomb_state.as_mut() {
            state.bomb_event_tick = Some(tick);
            state.bomb_state = CsgoDemoBombStatus::Exploded;
        }
    }
}

#[allow(dead_code)]
#[derive(Debug,Clone)]
pub struct CsgoDemoStringTable {
    name: String,
    max_entries: i32,
    user_data_size: i32,
    user_data_size_bits: i32,
    user_data_fixed_size: bool,
}

#[allow(dead_code)]
#[derive(Debug,Clone)]
pub struct CsgoDemoPlayerInfo {
    version: u64,
    // This is the same SteamID that we can find via GSI.
    pub xuid: u64,
    pub name: String,
    user_id: i32,
    guid: String,
    friends_id: u32,
    friends_name: String,
    fake_player: bool,
    is_hltv: bool,
    custom_files: Vec<u32>,
    files_downloaded: u8,
    entity_id: i32,
}

pub fn parse_csgo_demo_player_info(input: &[u8]) -> IResult<&[u8], CsgoDemoPlayerInfo> {
    let (input, version) = le_u64(input)?;
    let (input, xuid) = be_u64(input)?;
    let (input, name) = map_res(take(128usize), std::str::from_utf8)(input)?;
    let (input, user_id) = be_i32(input)?;
    let (input, guid) = map_res(take(33usize), std::str::from_utf8)(input)?;
    let (input, _p1) = take(3usize)(input)?;
    let (input, friends_id) = be_u32(input)?;
    let (input, friends_name) = map_res(take(128usize), std::str::from_utf8)(input)?;
    let (input, fake_player) = le_u8(input)?;
    let (input, is_hltv) = le_u8(input)?;
    let (input, _p2) = take(2usize)(input)?;
    let (input, custom_files) = many_m_n(4, 4, le_u32)(input)?;
    let (input, files_downloaded) = le_u8(input)?;
    let (input, _p3) = take(3usize)(input)?;

    Ok((input, CsgoDemoPlayerInfo{
        version,
        xuid,
        name: String::from(name).trim_end_matches(char::from(0)).to_string(),
        user_id,
        guid: String::from(guid).trim_end_matches(char::from(0)).to_string(),
        friends_id,
        friends_name: String::from(friends_name).trim_end_matches(char::from(0)).to_string(),
        fake_player: fake_player > 0,
        is_hltv: is_hltv > 0,
        custom_files,
        files_downloaded,
        // Entity ID is **NOT** serialized by Valve so we don't need to read it here.
        entity_id: -1,
    }))
}

// Note that this is not a byte-by-byte representation of the CS:GO demo
// (aside from the header). It's meant to be a slimmed down representation
// that extracts useful information out and presents it into a more useful
// manner.
#[derive(Debug)]
pub struct CsgoDemo {
    pub header: CsgoDemoHeader,
    pub game_start_tick: Option<i32>,
    pub rounds: Vec<CsgoDemoRound>,
    // These variables are only needed when parsing the demo file for handling
    // more intermediary states.
    string_tables: Vec<CsgoDemoStringTable>,
    pub player_info: HashMap<i32, CsgoDemoPlayerInfo>,
    model_precache: HashMap<i32, String>,
    pub entities: CsgoEntityScene,
}

impl Default for CsgoDemo {
    fn default() -> Self {
        Self {
            header: CsgoDemoHeader::default(),
            game_start_tick: None,
            rounds: vec![],
            string_tables: vec![],
            player_info: HashMap::new(),
            model_precache: HashMap::new(),
            entities: CsgoEntityScene::default(),
        }
    }
}

type CsgoParsedGameEventMessage = HashMap<String, csvc_msg_game_event::KeyT>;

fn parse_csgo_game_event_message(event: CsvcMsgGameEvent, desc: &csvc_msg_game_event_list::DescriptorT) -> Result<CsgoParsedGameEventMessage, SquadOvError> {
    let mut msg = HashMap::new();

    for (idx, key) in event.keys.into_iter().enumerate() {
        if let Some(descriptor_key) = desc.keys.get(idx) {
            let name = descriptor_key.name();
            msg.insert(name.to_string(), key);
        }
    }

    Ok(msg)
}

impl CsgoDemo {
    pub fn handle_game_event(&mut self, tick: i32, event: CsvcMsgGameEvent, desc: &csvc_msg_game_event_list::DescriptorT) -> Result<(), SquadOvError> {
        let event_name = desc.name();
        let current_round_idx = if self.rounds.is_empty() { 0 } else { self.rounds.len() - 1 };
        match event_name {
            "round_announce_match_start" => {
                log::debug!("csgo game start at: {} [{}]", tick, event_name);
                self.game_start_tick = Some(tick);
                // Freeze whatever round we're in because this gets "announced" after the freeze round.
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    round.round_frozen = true;
                }
            },
            "player_death" => {
                log::debug!("csgo player death at: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    if round.round_frozen {
                        // Determine who died and who killed them (and how they died).
                        // We want to keep these events associated with rounds.
                        let mut msg = parse_csgo_game_event_message(event, desc)?;
                        let kill = CsgoDemoKill{
                            tick,
                            victim: msg.remove("userid").ok_or(SquadOvError::NotFound)?.val_short(),
                            killer: msg.remove("attacker").map(|x| { x.val_short() }),
                            assister: msg.remove("assister").map(|x| { x.val_short() }),
                            flash_assist: msg.remove("assistedflash").map(|x| { x.val_bool() }).unwrap_or(false),
                            headshot: msg.remove("headshot").map(|x| { x.val_bool() }).unwrap_or(false),
                            smoke: msg.remove("thrusmoke").map(|x| { x.val_bool() }).unwrap_or(false),
                            blind: msg.remove("attackerblind").map(|x| { x.val_bool() }).unwrap_or(false),
                            wallbang: msg.remove("penetrated").map(|x| { x.val_short() }).unwrap_or(0) > 0,
                            noscope: msg.remove("noscope").map(|x| { x.val_bool() }).unwrap_or(false),
                            weapon: csgo_string_to_weapon(&msg.remove("weapon").map(|x| { String::from(x.val_string()) }).unwrap_or(String::new())),
                        };
                        round.kills.push(kill);
                    }
                }
            },
            "player_hurt" => {
                log::debug!("csgo player hurt at: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    if round.round_frozen {
                        let mut msg = parse_csgo_game_event_message(event, desc)?;
                        let damage = CsgoDemoDamage{
                            tick,
                            attacker: msg.remove("attacker").map(|x| { x.val_short() }),
                            receiver: msg.remove("userid").ok_or(SquadOvError::NotFound)?.val_short(),
                            remaining_health: msg.remove("health").map(|x| { x.val_byte() }).unwrap_or(0),
                            remaining_armor: msg.remove("armor").map(|x| { x.val_byte() }).unwrap_or(0),
                            damage_health: msg.remove("dmg_health").map(|x| { x.val_short() }).unwrap_or(0),
                            damage_armor: msg.remove("dmg_armor").map(|x| { x.val_byte() }).unwrap_or(0),
                            weapon: csgo_string_to_weapon(&msg.remove("weapon").map(|x| { String::from(x.val_string()) }).unwrap_or(String::new())),
                            hitgroup: CsgoDemoHitGroup::try_from(msg.remove("hitgroup").map(|x| { x.val_byte() }).unwrap_or(0))?,
                        };
                        round.damage.push(damage);
                    }
                }
            },
            "round_start" => {
                log::debug!("csgo round start at: {}", tick);

                // If the previous round hasn't been "frozen" then we want to overwrite it instead.
                // A frozen round has experienced a valid "round freeze end".
                let overwrite = !self.rounds.is_empty() && !self.rounds[self.rounds.len() - 1].round_frozen;

                // Create a new empty round - we assume that we've tracked
                // all the rounds properly so that the number of rounds in the
                // rounds vector is accurate.
                let mut new_round = CsgoDemoRound::default();
                new_round.round_num = if overwrite {
                    current_round_idx
                } else {
                    self.rounds.len()
                };
                new_round.round_start_tick = tick;

                // Populate player info.
                for (uid, _player) in &self.player_info {
                    new_round.players.insert(*uid, CsgoDemoRoundPlayerInfo::default());
                }

                if overwrite {
                    let idx = self.rounds.len() - 1;
                    self.rounds[idx] = new_round;
                } else {
                    self.rounds.push(new_round);
                }
            },
            "round_end" => {
                log::debug!("csgo round end at: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    round.round_end_tick = Some(tick);

                    // There's additional information that we can grab from
                    // the event regarding how the round was won and who won it.
                    let mut msg = parse_csgo_game_event_message(event, desc)?;
                    let msg_team_id = msg.remove("winner").ok_or(SquadOvError::NotFound)?.val_byte();
                    round.round_winner = if msg_team_id == self.entities.parse_state.ct_id {
                        Some(CsgoTeam::TeamCT)
                    } else if msg_team_id == self.entities.parse_state.terrorist_id {
                        Some(CsgoTeam::TeamT)
                    } else {
                        None
                    };
                    round.round_win_reason = CsgoRoundWin::try_from(msg.remove("reason").ok_or(SquadOvError::NotFound)?.val_byte()).ok();

                    // Pull round end match stats from the player's entity if available.
                    for (uid, player) in &self.player_info {
                        if let Some(player_entity) = self.entities.get_entity(player.entity_id) {
                            if let Some(player_round_info) = round.players.get_mut(uid) {
                                {
                                    let key = format!("m_iMatchStats_Deaths.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.deaths = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_Assists.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.assists = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_HeadShotKills.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.headshot_kills = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_Objective.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.objective = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_CashEarned.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.cash_earned = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_UtilityDamage.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.utility_damage = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_EnemiesFlashed.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.enemies_flashed = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_Kills.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.kills = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_Damage.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.damage = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_EquipmentValue.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.equipment_value = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_MoneySaved.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.money_saved = prop.value.v_i32.unwrap_or(0);
                                    }
                                }

                                {
                                    let key = format!("m_iMatchStats_KillReward.{:03}", current_round_idx);
                                    if let Some(prop) = player_entity.get_prop(&key) {
                                        player_round_info.kill_reward = prop.value.v_i32.unwrap_or(0);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "bomb_planted" => {
                log::debug!("csgo bomb planted: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    // Also want to know who planted the bomb and where.
                    let mut msg = parse_csgo_game_event_message(event, desc)?;

                    let bomb_site_id = msg.remove("site").ok_or(SquadOvError::NotFound)?.val_short();
                    let bomb_site = if let Some(site_a_id) = self.entities.parse_state.site_a_trigger {
                        if site_a_id == bomb_site_id {
                            CsgoDemoBombSite::SiteA
                        } else {
                            CsgoDemoBombSite::SiteB
                        }
                    } else if let Some(site_b_id) = self.entities.parse_state.site_b_trigger {
                        if site_b_id == bomb_site_id {
                            CsgoDemoBombSite::SiteB
                        } else {
                            CsgoDemoBombSite::SiteA
                        }
                    } else {
                        CsgoDemoBombSite::SiteUnknown
                    };

                    round.plant_bomb(
                        tick,
                        bomb_site,
                        msg.remove("userid").ok_or(SquadOvError::NotFound)?.val_short(),
                    );
                }
            },
            "bomb_defused" => {
                log::debug!("csgo bomb defused: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    // Also want to know who the bomb event is associated with (i.e. who defused).
                    // No need to grab the bomb location since it should be the same as where it was planted.
                    let mut msg = parse_csgo_game_event_message(event, desc)?;
                    round.defuse_bomb(tick, msg.remove("userid").ok_or(SquadOvError::NotFound)?.val_short());
                }
            },
            "bomb_exploded" => {
                log::debug!("csgo bomb exploded: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    round.explode_bomb(tick);
                }
            },
            "round_freeze_end" => {
                log::debug!("csgo round freeze end: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    if let Some(game_start_tick) = self.game_start_tick {
                        round.round_frozen = tick >= game_start_tick;
                    }

                    // There's gonna be an extra round_freeze_end when the game ends so we want to ignore that one.
                    if round.freeze_end_tick.is_some() {
                        return Ok(());
                    }

                    round.freeze_end_tick = Some(tick);
                    
                    // Also, at the end of the freeze round, scrape the player entity for information on what the
                    // user's weapons are.
                    for (uid, player) in &self.player_info {
                        if let Some(player_entity) = self.entities.get_entity(player.entity_id) {
                            let mut player_round_weapons: Vec<CsgoWeapon> = vec![];

                            for i in 0..CSGO_PLAYER_MAX_WEAPONS {
                                let weapon_key = format!("m_hMyWeapons.{:03}", i);
                                if let Some(weapon) = player_entity.get_prop(&weapon_key) {
                                    let weapon_entity_id = weapon.value.v_i32.unwrap_or(0) & CSGO_WEAPON_ID_MASK;
                                    // Grab the weapon entity itself and figure out what weapon it is based off its class.
                                    if let Some(weapon_entity) = self.entities.get_entity(weapon_entity_id) {
                                        if let Some(weapon_class) = self.entities.get_class_name(weapon_entity.class as i32)? {
                                            let weapon = weapon_entity.to_weapon(&weapon_class);

                                            player_round_weapons.push({
                                                let model_idx = weapon_entity.get_prop("m_nModelIndex").map(|x| { x.value.v_i32.unwrap_or(0) }).unwrap_or(0);
                                                let model_name = self.model_precache.get(&model_idx).map(|x| { x.as_str() }).unwrap_or("");
                                                // There's certain special cases where multiple weapons use the same class name.
                                                // In that case we further refine the actual weapon using the weapon model.
                                                // TODO: Maybe just use the model instead?
                                                match weapon {
                                                    CsgoWeapon::P2000 => if model_name.contains("pist_hkp2000") {
                                                        CsgoWeapon::P2000
                                                    } else if model_name.contains("pist_223") {
                                                        CsgoWeapon::Usps
                                                    } else {
                                                        log::info!("Unknown weapon model: {}, {} [p2000]", model_name, &weapon_class);
                                                        weapon
                                                    },
                                                    CsgoWeapon::M4a4 => if model_name.contains("rif_m4a1") {
                                                        CsgoWeapon::M4a4
                                                    } else if model_name.contains("rif_m4a1_s") {
                                                        CsgoWeapon::M4a1s
                                                    } else {
                                                        log::info!("Unknown weapon model: {}, {} [m4a4]", model_name, &weapon_class);
                                                        weapon
                                                    },
                                                    CsgoWeapon::P250 => if model_name.contains("pist_cz_75") {
                                                        CsgoWeapon::Cz75
                                                    } else if model_name.contains("pist_p250") {
                                                        CsgoWeapon::P250
                                                    } else {
                                                        log::info!("Unknown weapon model: {}, {} [p250]", model_name, &weapon_class);
                                                        weapon
                                                    },
                                                    CsgoWeapon::Deagle => if model_name.contains("pist_deagle") {
                                                        CsgoWeapon::Deagle
                                                    } else if model_name.contains("pist_revolver") {
                                                        CsgoWeapon::R8
                                                    } else {
                                                        log::info!("Unknown weapon model: {}, {} [deagle]", model_name, &weapon_class);
                                                        weapon
                                                    },
                                                    CsgoWeapon::Mp7 => if model_name.contains("smg_mp7") {
                                                        CsgoWeapon::Mp7
                                                    } else if model_name.contains("smg_mp5sd") {
                                                        CsgoWeapon::Mp5
                                                    } else {
                                                        log::info!("Unknown weapon model: {}, {} [mp7]", model_name, &weapon_class);
                                                        weapon
                                                    },
                                                    _ => weapon
                                                }
                                            });
                                        }
                                    }
                                }
                            }

                            if let Some(player_round_info) = round.players.get_mut(uid) {
                                player_round_info.weapons = player_round_weapons;

                                if let Some(armor_prop) = player_entity.get_prop("m_ArmorValue") {
                                    player_round_info.armor = armor_prop.value.v_i32.unwrap_or(0);
                                }

                                if let Some(defuse_prop) = player_entity.get_prop("m_bHasDefuser") {
                                    player_round_info.has_defuse = defuse_prop.value.v_i32.unwrap_or(0) != 0;
                                }

                                if let Some(helmet_prop) = player_entity.get_prop("m_bHasHelmet") {
                                    player_round_info.has_helmet = helmet_prop.value.v_i32.unwrap_or(0) != 0;
                                }

                                if let Some(team_prop) = player_entity.get_prop("m_iTeamNum") {
                                    let team_id = team_prop.value.v_i32.unwrap_or(-1);
                                    player_round_info.team = if team_id == self.entities.parse_state.ct_id {
                                        CsgoTeam::TeamCT
                                    } else if team_id == self.entities.parse_state.terrorist_id {
                                        CsgoTeam::TeamT
                                    } else {
                                        CsgoTeam::TeamSpectate
                                    };
                                }

                                if let Some(account_prop) = player_entity.get_prop("m_iAccount") {
                                    player_round_info.money = account_prop.value.v_i32.unwrap_or(0);
                                }
                            }
                        }
                    }
                }
            },
            "round_mvp" => {
                log::debug!("csgo round mvp: {}", tick);
                if let Some(round) = self.rounds.get_mut(current_round_idx) {
                    // Need to determine who exactly got the MVP. This **should** happen before the 
                    // next "round_start" event.
                    let mut msg = parse_csgo_game_event_message(event, desc)?;
                    round.round_mvp = Some(msg.remove("userid").ok_or(SquadOvError::NotFound)?.val_short());
                }
            },
            _ => (),
        };
        Ok(())
    }

    fn generic_handle_string_table_change(&mut self, table: &CsgoDemoStringTable, num_entries: i32, data: &[u8]) -> Result<(), SquadOvError> {
        let mut reader = BitReader::new(data);
        let encode_using_dictionaries = reader.read_bit()?;
        if encode_using_dictionaries {
            log::warn!("Skip dictionary-encoded string table.");
            return Ok(());
        }

        let mut last_entry = -1;

        // Forces a log2 on max_entries (on the table) to get
        // the number of entry bits.
        let num_entry_bits = crate::math::integer_log2(table.max_entries) as usize;

        let mut history : VecDeque<String> = VecDeque::new();
        for _i in 0..num_entries {
            last_entry = if reader.read_bit()? {
                last_entry + 1
            } else {
                reader.read_multibit::<u32>(num_entry_bits)? as i32
            };

            if last_entry < 0 || last_entry >= table.max_entries {
                log::warn!("Bad string table index: {} [{}]", last_entry, table.max_entries);
                return Ok(());
            }

            // Obtain entry/substring keys.
            let mut entry: String = String::new();

            if reader.read_bit()? {
                if reader.read_bit()? {
                    // We have a substring in addition to the entry.
                    let index = reader.read_multibit::<usize>(5)?;
                    if index >= history.len() {
                        log::warn!("Invalid history index: {} vs {}", index, history.len());
                        break;
                    }

                    let bytes_to_copy = reader.read_multibit::<usize>(SUBSTRING_BITS)?;
                    entry = String::from(&history.get(index).unwrap()[0..bytes_to_copy]);
                    let substr = reader.read_null_terminated_str()?; 
                    entry.push_str(&substr);
                } else {
                    // Just a single string entry.
                    entry = reader.read_null_terminated_str()?;
                }
            }

            // Now obtain user data.
            let mut user_data: Option<Vec<u8>> = None;
            if reader.read_bit()? {
                let bits_to_read = if table.user_data_fixed_size {
                    table.user_data_size_bits as usize
                } else {
                    reader.read_multibit::<usize>(MAX_USERDATA_BITS)? * 8
                };

                user_data = Some(reader.read_raw(bits_to_read)?);
            }

            if table.name == "modelprecache" {
                self.model_precache.insert(last_entry, entry.clone());
            } else if let Some(raw_user_data) = user_data {
                if table.name == "userinfo" {
                    let mut player_info = parse_csgo_demo_player_info(&raw_user_data)?.1;
                    // WHY THE FUCK IS THIS +1 VALVE. In their reference code, they set "last_entry"
                    // to be the stored player info's entity ID. BUT! In the ShowPlayerInfo function, when they
                    // go search for the entity using this stored value they add 1. HOLY SHIT.
                    player_info.entity_id = last_entry + 1;
                    log::debug!("Player Info: {:?}", player_info);

                    // Use user_id instead of entity_id because all the events and what not
                    // will refer to the player's userid and not entity id.
                    self.player_info.insert(player_info.user_id, player_info);
                }
            }

            if history.len() > 31 {
                history.pop_front();
            }

            history.push_back(entry);
        }

        Ok(())
    }

    pub fn handle_string_table_create(&mut self, data: CsvcMsgCreateStringTable) -> Result<(), SquadOvError> {
        let table = CsgoDemoStringTable{
            name: data.name.ok_or(SquadOvError::NotFound)?,
            max_entries: data.max_entries.ok_or(SquadOvError::NotFound)?,
            user_data_size: data.user_data_size.ok_or(SquadOvError::NotFound)?,
            user_data_size_bits: data.user_data_size_bits.ok_or(SquadOvError::NotFound)?,
            user_data_fixed_size: data.user_data_fixed_size.ok_or(SquadOvError::NotFound)?,
        };

        self.generic_handle_string_table_change(&table, data.num_entries.ok_or(SquadOvError::NotFound)?, &data.string_data.ok_or(SquadOvError::NotFound)?)?;
        self.string_tables.push(table);
        Ok(())
    }

    pub fn handle_string_table_update(&mut self, data: CsvcMsgUpdateStringTable) -> Result<(), SquadOvError> {
        // Check that we're modifying an appropriate table:
        //  1) The table index (id) must exist.
        //  2) We shouldn't be changing more entries than can possibly exist.
        
        // Would really prefer not to clone here but can't figure out how to get it to play nicely with the borrow checker.
        let src_table = self.string_tables.get(data.table_id.ok_or(SquadOvError::NotFound)? as usize).cloned();
        if let Some(table) = src_table {
            let num_changed = data.num_changed_entries.ok_or(SquadOvError::NotFound)?;
            if num_changed < table.max_entries {
                self.generic_handle_string_table_change(&table, num_changed, &data.string_data.ok_or(SquadOvError::NotFound)?)?;
            } else {
                log::warn!("String table changing more entries than max.");
            }
        } else {
            log::warn!("Table ID does not exist...skipping.");
        }

        Ok(())
    }

    pub fn on_data_table(&mut self, table: Arc<RwLock<CsgoDemoDataTable>>) -> Result<(), SquadOvError> {
        self.entities.connect_data_table(table);
        Ok(())
    }

    pub fn handle_entity_update(&mut self, data: CsvcMsgPacketEntities) -> Result<(), SquadOvError> {
        self.entities.handle_entity_update(data)?;
        Ok(())
    }
}