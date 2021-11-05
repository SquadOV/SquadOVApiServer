use serde::Deserialize;
use chrono::{DateTime, Utc};
use sqlx::{Transaction, Postgres, Row};
use uuid::Uuid;
use std::convert::TryInto;
use std::cmp::PartialEq;
use std::str::FromStr;
use crate::{
    SquadOvError,
    wow::COMBATLOG_FILTER_ME,
};
use unicode_segmentation::UnicodeSegmentation;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
pub struct WoWCombatLogState {
    #[serde(rename="combatLogVersion")]
    pub combat_log_version: String,
    #[serde(rename="advancedLog")]
    pub advanced_log: bool,
    #[serde(rename="buildVersion")]
    pub build_version: String
}

pub struct FullWoWCombatLogState {
    pub state: WoWCombatLogState,
}

#[derive(Deserialize)]
pub struct RawWoWCombatLogPayload {
    pub timestamp: DateTime<Utc>,
    pub parts: Vec<String>,
    #[serde(rename="rawLog")]
    pub raw_log: Option<String>,
    #[serde(rename="logLine")]
    pub log_line: i64,
    pub version: i32,
}

fn split_wow_combat_log_tokens(full_log: &str) -> Vec<String> {
    let mut new_parts: Vec<String> = Vec::new();

    let log_parts: Vec<&str> = full_log.graphemes(true).collect();
    let mut log_iter = log_parts.iter();

    let mut stack: Vec<&str> = vec![","];
    let mut current_part: Vec<&str> = Vec::new();
    while let Some(ch) = log_iter.next() {
        // There are generally 3 cases here when we first start off (i.e.
        // we're coming clean off processing an ending comma)
        // 1) We encounter a [ or a (
        // 2) We encounter a "
        // 3) We encounter any other character.
        // In cases 1 and 3, we want to grab the string that goes from the
        // current position up to the character before the relevant closing comma.
        // In case 2, we want to grab the string that goes from the next position up
        // to the character before the closing quotation.
        //
        // To accomplish this, we keep track of a "stack" of special symbols that will
        // represent some sort of a nested string, i.e. [], (), "". We also keep track of
        // all top-level commas in the stack (a top-level comma is defined to be a comma
        // that is detected when the stack is empty). Each comma will perform a pop AND a push
        // operation as it simultaneously marks the ending of the previous stack and the start
        // of a new stack. We only detect a new part when the stack is empty.
        //
        // For example, assume are we given the following string
        //
        //     ["TEST"],
        //
        // We implicitly assume that the string starts with a comma to begin the stack.
        // We detect a [ so push it onto the stack. Then we detect a " and we push it onto
        // the stack (only the [ gets tracked as part of the current part). Each character
        // of TEST is non-special and thus collected as part of the current part (i.e [TEST ).
        // Finally, we detect " and then ] which pop " and [ off the stack respectively. Finally
        // we detect the final comma which pops the first comma off the stack so that we end up
        // with a part of [TEST]. The only cases where a special character is not added to the
        // current part is in the case of a top-level comma and quotation marks.

        match *ch {
            "[" | "(" => {
                current_part.push(ch);
                stack.push(ch);
            },
            "]" | ")" => {
                let last = stack.pop().unwrap_or("");
                if (*ch == "]" && last != "[") || (*ch == ")" && last != "(") {
                    log::warn!("WoW Log Mismatched Char: {} vs {} - {}", ch, last, full_log);
                    break;
                }
                current_part.push(ch)
            },
            "\"" => {
                // Note that we need to differentiate between quotation marks that
                // start a stack and quotation marks that end a stack. If the current
                // element on the stack isn't a quotation mark then we assume that it
                // starts the stack and if it is a quotation mark then we assume that
                // it ends the stack. This assumption rests on the fact that I believe
                // that it isn't possible for any string in the combat log to look like
                // "\"".
                let last = stack.last();
                if last.is_none() {
                    log::warn!("Empty stack.");
                    break;
                }

                if last.unwrap() == ch {
                    stack.pop();
                } else {
                    stack.push(ch);
                }
            },
            "," => {
                if stack.len() == 1 && stack[0] == "," {
                    new_parts.push(String::from(current_part.join("")));
                    current_part.clear();
                } else {
                    current_part.push(ch)
                }
            },
            _ => current_part.push(ch)
        }          
    }

    // Implicit comma at the end of the string
    new_parts.push(String::from(current_part.join("")));
    new_parts
}

impl RawWoWCombatLogPayload {
    pub fn flatten(&self) -> String {
        format!(
            "{} {}\n",
            self.timestamp.format("%m/%d %T").to_string(),
            if let Some(raw) = &self.raw_log {
                raw.clone()
            } else {
                self.parts.join(",")
            },
        )
    }

    pub fn redo_parts(&mut self) {
        // I'm not really sure if we need to do unicode aware stepping here
        // but it's probably better safe than sorry.
        self.parts = split_wow_combat_log_tokens(&if let Some(raw) = &self.raw_log {
            raw.clone()
        } else {
            self.parts.join(",")
        });
    }

    pub fn is_finish_token(&self) -> bool {
        self.parts.len() > 0 && self.parts[0] == "SQUADOV_END_COMBAT_LOG"
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct WoWSpellInfo {
    id: i64,
    name: String,
    school: i64
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct WowCovenantInfo {
    covenant_id: i32,
    soulbind_id: i32,
    soulbind_traits: Vec<i32>,
    conduits: Vec<WoWItemInfo>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub enum WoWDamageType {
    SwingDamage,
    SpellDamage(WoWSpellInfo)
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub enum WoWSpellAuraType {
    Buff,
    Debuff,
    Unknown
}

impl FromStr for WoWSpellAuraType {
    type Err = SquadOvError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "BUFF" => Ok(WoWSpellAuraType::Buff),
            "DEBUFF" => Ok(WoWSpellAuraType::Debuff),
            _ => Err(SquadOvError::BadRequest)
        }
    }
}

impl ToString for WoWSpellAuraType {
    fn to_string(&self) -> String {
        String::from(match self {
            WoWSpellAuraType::Buff => "BUFF",
            WoWSpellAuraType::Debuff => "DEBUFF",
            WoWSpellAuraType::Unknown => "UNKNOWN"
        })
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct WoWItemInfo {
    item_id: i64,
    ilvl: i32
}

fn parse_wow_item_info_from_str(s: &str) -> Result<Vec<WoWItemInfo>, SquadOvError> {
    if s.len() < 2 {
        return Ok(vec![]);
    }

    let tokens = split_wow_combat_log_tokens(&s[1..s.len()-1]);
    // Each top level token is for one given item.
    Ok(tokens.into_iter().map(|x| {
        if x.len() < 2 {
            return WoWItemInfo {
                item_id: -1,
                ilvl: -1,
            }
        }

        let item_parts = split_wow_combat_log_tokens(&x[1..x.len()-1]);
        WoWItemInfo{
            item_id: item_parts[0].parse().unwrap_or(-1),
            ilvl: item_parts[1].parse().unwrap_or(-1),
        }
    }).collect())
}

fn parse_wow_talents_from_str(s: &str) -> Result<Vec<i32>, SquadOvError> {
    if s.len() < 2 {
        return Ok(vec![]);
    }
    
    let tokens = split_wow_combat_log_tokens(&s[1..s.len()-1]);
    Ok(tokens.into_iter().map(|x| {
        Ok(x.parse()?)
    }).collect::<Result<Vec<i32>, SquadOvError>>()?)
}

fn parse_soulbind_traits_from_str(s: &str) -> Result<Vec<i32>, SquadOvError> {
    if s.len() < 2 {
        return Ok(vec![]);
    }

    let tokens = split_wow_combat_log_tokens(&s[1..s.len()-1]);
    Ok(tokens.into_iter().map(|x| {
        if x.len() < 2 {
            return Ok(0);
        }
        let inner = split_wow_combat_log_tokens(&x[1..x.len()-1]);
        Ok(inner[0].parse()?)
    }).collect::<Result<Vec<i32>, SquadOvError>>()?)
}

fn parse_wow_covenant_from_str(s: &str) -> Result<Option<WowCovenantInfo>, SquadOvError> {
    if s.len() < 2 {
        return Ok(None);
    }

    let tokens = split_wow_combat_log_tokens(&s[1..s.len()-1]);
    Ok(
        Some(
            WowCovenantInfo{
                covenant_id: tokens[1].parse()?,
                soulbind_id: tokens[0].parse()?,
                soulbind_traits: parse_soulbind_traits_from_str(&tokens[3])?,
                conduits: parse_wow_item_info_from_str(&tokens[4])?
            }
        )
    )
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct WowClassUpdateFromSpell {
    player_guid: String,
    spell_id: i64,
    // The latter two are necessary if we want to update the user character cache too.
    // We will only do that in the case where user_id is not None.
    player_name: String,
    user_id: Option<i64>,
    build_version: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub enum WoWCombatLogEventType {
    UnitDied{
        unconcious: bool,
    },
    DamageDone{
        damage: WoWDamageType,
        amount: i64,
        overkill: i64,
    },
    Healing{
        spell: WoWSpellInfo,
        amount: i64,
        overheal: i64,
        absorbed: i64,
    },
    Resurrect(WoWSpellInfo),
    SpellAura{
        spell: WoWSpellInfo,
        aura_type: WoWSpellAuraType,
        applied: bool
    },
    AuraBreak{
        aura: WoWSpellInfo,
        spell: Option<WoWSpellInfo>,
        aura_type: WoWSpellAuraType,
    },
    SpellCast{
        spell: WoWSpellInfo,
        start: bool,
        finish: bool,
        success: bool,
    },
    SpellSummon(WoWSpellInfo),
    CombatantInfo{
        guid: String,
        team: i32,
        strength: i32,
        agility: i32,
        stamina: i32,
        intelligence: i32,
        armor: i32,
        spec_id: i32,
        talents: Vec<i32>,
        pvp_talents: Vec<i32>,
        covenant: Option<WowCovenantInfo>,
        items: Vec<WoWItemInfo>,
        rating: i32,
    },
    EncounterStart{
        encounter_id: i32,
        encounter_name: String,
        difficulty: i32,
        num_players: i32,
        instance_id: i32
    },
    EncounterEnd{
        encounter_id: i32,
        encounter_name: String,
        difficulty: i32,
        num_players: i32,
        success: bool
    },
    ArenaStart{
        instance_id: i32,
        arena_type: String,
        local_team_id: i32,
    },
    ArenaEnd{
        winning_team_id: i32,
        match_duration_seconds: i32,
        new_ratings: Vec<i32>,
    },
    ChallengeModeStart{
        challenge_name: String,
        instance_id: i32,
        keystone: i32
    },
    ChallengeModeEnd{
        instance_id: i32,
        success: bool,
        keystone: i32,
        time_ms: i64
    },
    ClassUpdateFromSpell(WowClassUpdateFromSpell),
    Unknown
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct WoWCombatLogSourceDest {
    guid: String,
    name: String,
    flags: i64,
    raid_flags: i64,
}

impl WoWCombatLogSourceDest {
    fn new(data: &[String; 4]) -> Result<Self, crate::SquadOvError> {
        if !data[2].starts_with("0x") || !data[3].starts_with("0x") {
            return Err(crate::SquadOvError::BadRequest);
        }

        Ok(Self {
            guid: data[0].clone(),
            name: data[1].clone(),
            flags: i64::from_str_radix(&data[2][2..], 16)?,
            raid_flags: i64::from_str_radix(&data[3][2..], 16)?,
        })
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub enum WoWGenericLevel {
    ItemLevel{
        level: i32
    },
    UnitLevel{
        level: i32
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub struct WoWCombatLogAdvancedCVars {
    unit_guid: String,
    owner_guid: String,
    current_hp: i64,
    max_hp: i64,
    attack_power: i64,
    spell_power: i64,
    armor: i64,
    unk1: i64,
    resource_type: Vec<i32>,
    current_resource: Vec<i64>,
    max_resource: Vec<i64>,
    resource_cost: Vec<i64>,
    coord0: f64,
    coord1: f64,
    map_id: i64,
    facing: f64,
    level: WoWGenericLevel
}

impl WoWCombatLogAdvancedCVars {
    fn new(data: &[String], use_ilvl: bool) -> Result<Self, crate::SquadOvError> {
        let has_armor = data.len() == 17;

        let level: i32 = data[if has_armor { 16 } else { 15 }].parse()?;
        let resource_type: Vec<i32> = data[if has_armor { 8 } else { 7 }].split('|').map(|x| {
            Ok(x.parse()?)
        }).collect::<Result<Vec<i32>, SquadOvError>>()?;
        let current_resource: Vec<i64> = data[if has_armor { 9 } else { 8 }].split('|').map(|x| {
            Ok(x.parse()?)
        }).collect::<Result<Vec<i64>, SquadOvError>>()?;
        let max_resource: Vec<i64> = data[if has_armor { 10 } else { 9 }].split('|').map(|x| {
            Ok(x.parse()?)
        }).collect::<Result<Vec<i64>, SquadOvError>>()?;
        let resource_cost: Vec<i64> = data[if has_armor { 11 } else { 10 }].split('|').map(|x| {
            Ok(x.parse()?)
        }).collect::<Result<Vec<i64>, SquadOvError>>()?;

        Ok(Self {
            unit_guid: data[0].clone(),
            owner_guid: data[1].clone(),
            current_hp: data[2].parse()?,
            max_hp: data[3].parse()?,
            attack_power: data[4].parse()?,
            spell_power: data[5].parse()?,
            armor: if has_armor { data[6].parse()? } else { 0 },
            unk1: data[if has_armor { 7 } else { 6}].parse()?,
            resource_type,
            current_resource,
            max_resource,
            resource_cost,
            coord0: data[if has_armor { 12 } else { 11} ].parse()?,
            coord1: data[if has_armor { 13 } else { 12} ].parse()?,
            map_id: data[if has_armor { 14 } else { 13 }].parse()?,
            facing: data[if has_armor { 15 } else { 14} ].parse()?,
            level: if use_ilvl {
                WoWGenericLevel::ItemLevel{level}
            } else {
                WoWGenericLevel::UnitLevel{level}
            }
        })
    }
}

#[derive(Clone,Debug)]
pub struct WoWCombatLogEvent {
    view_id: Uuid,
    alt_view_id: i64,
    user_id: i64,
    log_line: i64,
    timestamp: DateTime<Utc>,
    source: Option<WoWCombatLogSourceDest>,
    dest: Option<WoWCombatLogSourceDest>,
    advanced: Option<WoWCombatLogAdvancedCVars>,
    pub event: WoWCombatLogEventType
}

pub fn parse_advanced_cvars_and_event_from_wow_combat_log(state: &WoWCombatLogState, payload: &RawWoWCombatLogPayload, user_id: i64) -> Result<(Option<WoWCombatLogAdvancedCVars>, WoWCombatLogEventType, Option<WowClassUpdateFromSpell>), crate::SquadOvError> {
    let action_parts: Vec<&str> = payload.parts[0].split("_").collect();
    let cl_version: i32 = state.combat_log_version.parse::<i32>()?;
    let advanced_cvar_offset: usize = if cl_version == 9 { 16 } else { 17 };

    match action_parts[0] {
        "SPELL" => {
            match action_parts[1] {
                // Need to handle ABSORBED separately since its format is different from other spells.
                "ABSORBED" => Ok((None, WoWCombatLogEventType::Unknown, None)),
                _ => {
                    if payload.parts.len() < 12 {
                        return Ok((None, WoWCombatLogEventType::Unknown, None));
                    }

                    let mut idx = 9;

                    if !payload.parts[idx+2].starts_with("0x") {
                        log::warn!("Invalid spell school: {}", payload.flatten());
                        return Ok((None, WoWCombatLogEventType::Unknown, None));
                    }

                    let spell_info = WoWSpellInfo{
                        id: payload.parts[idx].parse()?,
                        name: payload.parts[idx+1].clone(),
                        school: i64::from_str_radix(&payload.parts[idx+2][2..], 16)?,
                    };
                    idx += 3;
                    
                    match payload.parts[0].as_str() {
                        "SPELL_DAMAGE" | "SPELL_PERIODIC_DAMAGE" => Ok({
                            let advanced = if state.advanced_log {
                                Some(WoWCombatLogAdvancedCVars::new(&payload.parts[idx..idx+advanced_cvar_offset], payload.parts[0].as_str() == "SPELL_DAMAGE")?)
                            } else {
                                None
                            };
                            idx += advanced_cvar_offset;
        
                            (advanced, WoWCombatLogEventType::DamageDone{
                                damage: WoWDamageType::SpellDamage(spell_info),
                                amount: payload.parts[idx].parse()?,
                                overkill: payload.parts[idx+1].parse()?,    
                            }, None)
                        }),
                        "SPELL_HEAL" | "SPELL_PERIODIC_HEAL" => Ok({
                            let advanced = if state.advanced_log {
                                Some(WoWCombatLogAdvancedCVars::new(&payload.parts[idx..idx+advanced_cvar_offset], true)?)
                            } else {
                                None
                            };
                            idx += advanced_cvar_offset;
        
                            (advanced, WoWCombatLogEventType::Healing{
                                spell: spell_info,
                                amount: payload.parts[idx+1].parse()?,
                                overheal: payload.parts[idx+2].parse()?,
                                absorbed: payload.parts[idx+3].parse()?,
                            }, None)
                        }),
                        "SPELL_RESURRECT" => Ok((None, WoWCombatLogEventType::Resurrect(spell_info), None)),
                        "SPELL_AURA_APPLIED" => Ok((None, WoWCombatLogEventType::SpellAura{
                            spell: spell_info,
                            aura_type: WoWSpellAuraType::from_str(&payload.parts[idx])?,
                            applied: true
                        }, None)),
                        "SPELL_AURA_REMOVED" => Ok((None, WoWCombatLogEventType::SpellAura{
                            spell: spell_info,
                            aura_type: WoWSpellAuraType::from_str(&payload.parts[idx])?,
                            applied: false
                        }, None)),
                        "SPELL_SUMMON" => Ok((None, WoWCombatLogEventType::SpellSummon(spell_info), None)),
                        "SPELL_AURA_BROKEN_SPELL" => Ok({
                            let by_spell = WoWSpellInfo{
                                id: payload.parts[idx].parse()?,
                                name: payload.parts[idx+1].clone(),
                                // Not sure why but this one is decimal instead of hex.
                                school: i64::from_str_radix(&payload.parts[idx+2][..], 10)?,
                            };
                            idx += 3;

                            (None, WoWCombatLogEventType::AuraBreak{
                                aura: spell_info,
                                spell: Some(by_spell),
                                aura_type: WoWSpellAuraType::from_str(&payload.parts[idx])?,
                            }, None)
                        }),
                        "SPELL_AURA_BROKEN" => Ok((None, WoWCombatLogEventType::AuraBreak{
                            aura: spell_info,
                            spell: None,
                            aura_type: WoWSpellAuraType::from_str(&payload.parts[idx])?,
                        }, None)),
                        "SPELL_CAST_START" => Ok((None, WoWCombatLogEventType::SpellCast{
                            spell: spell_info,
                            start: true,
                            finish: false,
                            success: false,
                        }, None)),
                        "SPELL_CAST_SUCCESS" => Ok({
                            let advanced = if state.advanced_log {
                                Some(WoWCombatLogAdvancedCVars::new(&payload.parts[idx..idx+advanced_cvar_offset], true)?)
                            } else {
                                None
                            };
                            
                            let spell_id = spell_info.id;
                            (
                                advanced,
                                WoWCombatLogEventType::SpellCast{
                                    spell: spell_info,
                                    start: false,
                                    finish: true,
                                    success: true,
                                },
                                // We need to update the class from the spell cast for instances where we either
                                // don't have spec ids (wow classic) or for cases where no combatant info is otherwise
                                // generated in the combat logs (arenas in classic, RBGs). We don't want to do this for non-player characters
                                // so we want to compare the player flags and make sure it ANDs againt 0x500 correctly which is
                                // 0x100 -> controlled by player and 0x400 -> type player.
                                {
                                    let flags = i64::from_str_radix(&payload.parts[3][2..], 16)?;
                                    if flags & 0x500 == 0x500 {
                                        Some(WowClassUpdateFromSpell{
                                            player_guid: payload.parts[1].clone(),
                                            player_name: payload.parts[2].clone(),
                                            user_id: if flags & COMBATLOG_FILTER_ME == COMBATLOG_FILTER_ME {
                                                Some(user_id)
                                            } else {
                                                None
                                            },
                                            spell_id,
                                            build_version: state.build_version.clone(),
                                        })
                                    } else { 
                                        None
                                    }
                                }
                            )
                        }),
                        "SPELL_CAST_FAILED" => Ok((None, WoWCombatLogEventType::SpellCast{
                            spell: spell_info,
                            start: false,
                            finish: true,
                            success: false,
                        }, None)),
                        _ => Ok((None, WoWCombatLogEventType::Unknown, None)),
                    }
                }
            }
        },
        _ => match payload.parts[0].as_str() {
            "UNIT_DIED" => Ok((None, WoWCombatLogEventType::UnitDied{
                unconcious: if cl_version == 9 { false } else { payload.parts[9] == "1" },
            }, None)),
            "SWING_DAMAGE_LANDED" => if payload.parts.len() >= 9 {
                Ok({
                    let mut idx = 9;
                    let advanced = if state.advanced_log {
                        Some(WoWCombatLogAdvancedCVars::new(&payload.parts[idx..idx+advanced_cvar_offset], false)?)
                    } else {
                        None
                    };
                    idx += advanced_cvar_offset;

                    (advanced, WoWCombatLogEventType::DamageDone{
                        damage: WoWDamageType::SwingDamage,
                        amount: payload.parts[idx].parse()?,
                        overkill: payload.parts[idx+1].parse()?,
                    }, None)
                })
            } else {
                log::warn!("Bad SWING_DAMAGE_LANDED line: {} - {} @ {}", payload.flatten(), payload.parts.len(), cl_version);
                Ok((None, WoWCombatLogEventType::Unknown, None))
            },
            "COMBATANT_INFO" =>  if cl_version == 9 && payload.parts.len() >= 31 {
                Ok((None, WoWCombatLogEventType::CombatantInfo{
                    guid: payload.parts[1].clone(),
                    team: payload.parts[2].parse()?,
                    strength: payload.parts[3].parse()?,
                    agility: payload.parts[4].parse()?,
                    stamina: payload.parts[5].parse()?,
                    intelligence: payload.parts[6].parse()?,
                    armor: 0,
                    spec_id: 0,
                    talents: vec![],
                    pvp_talents: vec![],
                    covenant: None,
                    items: parse_wow_item_info_from_str(&payload.parts[27])?,
                    rating: payload.parts[30].parse()?,
                }, None))
            } else if payload.parts.len() >= 33 {
                Ok((None, WoWCombatLogEventType::CombatantInfo{
                    guid: payload.parts[1].clone(),
                    team: payload.parts[2].parse()?,
                    strength: payload.parts[3].parse()?,
                    agility: payload.parts[4].parse()?,
                    stamina: payload.parts[5].parse()?,
                    intelligence: payload.parts[6].parse()?,
                    armor: payload.parts[23].parse()?,
                    spec_id: payload.parts[24].parse()?,
                    talents: parse_wow_talents_from_str(&payload.parts[25])?,
                    pvp_talents: parse_wow_talents_from_str(&payload.parts[26])?,
                    covenant: parse_wow_covenant_from_str(&payload.parts[27])?,
                    items: parse_wow_item_info_from_str(&payload.parts[28])?,
                    rating: payload.parts[32].parse()?,
                }, None))
            } else {
                log::warn!("Bad COMBATANT_INFO line: {} - {} @ {}", payload.flatten(), payload.parts.len(), cl_version);
                Ok((None, WoWCombatLogEventType::Unknown, None))
            },
            "ENCOUNTER_START" => Ok((None, WoWCombatLogEventType::EncounterStart{
                encounter_id: payload.parts[1].parse()?,
                encounter_name: payload.parts[2].clone(),
                difficulty: payload.parts[3].parse()?,
                num_players: payload.parts[4].parse()?,
                instance_id: payload.parts[5].parse()?,
            }, None)),
            "ENCOUNTER_END" => Ok((None, WoWCombatLogEventType::EncounterEnd{
                encounter_id: payload.parts[1].parse()?,
                encounter_name: payload.parts[2].clone(),
                difficulty: payload.parts[3].parse()?,
                num_players: payload.parts[4].parse()?,
                success: payload.parts[5] == "1"
            }, None)),
            "CHALLENGE_MODE_START" => Ok((None, WoWCombatLogEventType::ChallengeModeStart{
                challenge_name: payload.parts[1].clone(),
                instance_id: payload.parts[2].parse()?,
                keystone: payload.parts[4].parse()?,
            }, None)),
            "CHALLENGE_MODE_END" => Ok((None, WoWCombatLogEventType::ChallengeModeEnd{
                instance_id: payload.parts[1].parse()?,
                success: payload.parts[2] == "1",
                keystone: payload.parts[3].parse()?,
                time_ms: payload.parts[4].parse()?,
            }, None)),
            "ARENA_MATCH_START" => Ok((None, WoWCombatLogEventType::ArenaStart{
                instance_id: payload.parts[1].parse()?,
                arena_type: payload.parts[3].clone(),
                local_team_id: payload.parts[4].parse()?,
            }, None)),
            "ARENA_MATCH_END" => Ok((None, WoWCombatLogEventType::ArenaEnd{
                winning_team_id: payload.parts[1].parse()?,
                match_duration_seconds: payload.parts[2].parse()?,
                new_ratings: vec![
                    payload.parts[3].parse()?,
                    payload.parts[4].parse()?,
                ],
            }, None)),
            _ => Ok((None, WoWCombatLogEventType::Unknown, None))
        },
    }
}

pub fn parse_raw_wow_combat_log_payload(uuid: &Uuid, alt_id: i64, user_id: i64, state: &WoWCombatLogState, payload: &RawWoWCombatLogPayload) -> Result<Vec<WoWCombatLogEvent>, crate::SquadOvError> {
    let (advanced, event, class_update) = parse_advanced_cvars_and_event_from_wow_combat_log(state, payload, user_id).unwrap_or((None, WoWCombatLogEventType::Unknown, None));
    if event == WoWCombatLogEventType::Unknown {
        return Ok(vec![])
    }

    let has_source_dest = match payload.parts[0].as_str() {
        "COMBAT_LOG_VERSION" |
            "ZONE_CHANGE" |
            "MAP_CHANGE" |
            "ENCOUNTER_START" |
            "ENCOUNTER_END" |
            "CHALLENGE_MODE_START" |
            "CHALLENGE_MODE_END" |
            "ARENA_MATCH_START" |
            "ARENA_MATCH_END" |
            "COMBATANT_INFO" => false,
        _ => true,
    };

    let mut ret = vec![WoWCombatLogEvent{
        view_id: uuid.clone(),
        alt_view_id: alt_id,
        user_id,
        log_line: payload.log_line,
        timestamp: payload.timestamp.clone(),
        source: if has_source_dest {
            if let Ok(x) = WoWCombatLogSourceDest::new(payload.parts[1..5].try_into()?) {
                Some(x)
            } else {
                log::warn!("Failed to parse source for WoW combat log payload: {}", payload.flatten());
                None
            }
        } else {
            None
        },
        dest: if has_source_dest {
            if let Ok(x) = WoWCombatLogSourceDest::new(payload.parts[5..9].try_into()?) {
                Some(x)
            } else {
                log::warn!("Failed to parse dest for WoW combat log payload: {}", payload.flatten());
                None
            }
        } else {
            None
        },
        advanced,
        event
    }];

    if let Some(update) = class_update {
        ret.push(WoWCombatLogEvent{
            view_id: uuid.clone(),
            alt_view_id: alt_id,
            user_id,
            log_line: 0,
            timestamp: payload.timestamp.clone(),
            source: None,
            dest: None,
            advanced: None,
            event: WoWCombatLogEventType::ClassUpdateFromSpell(update),
        });
    }

    Ok(ret)
}

async fn insert_missing_wow_character_presence_for_events(tx: &mut Transaction<'_, Postgres>, events: &[WoWCombatLogEvent]) -> Result<(), SquadOvError> {
    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_character_presence (
            view_id,
            unit_guid,
            unit_name,
            owner_guid,
            flags,
            has_combatant_info
        )
        VALUES
    "));

    let mut added = 0;
    for e in events {
        let owner_guid;
        let pet_guid;
        
        if let Some(advanced) = &e.advanced {
            if advanced.owner_guid != crate::NIL_WOW_GUID && advanced.unit_guid != crate::NIL_WOW_GUID {
                owner_guid = Some(advanced.owner_guid.clone());
                pet_guid = Some(advanced.unit_guid.clone());
            } else {
                owner_guid = None;
                pet_guid = None;
            }
        } else {
            owner_guid = None;
            pet_guid = None;
        };

        if let Some(source) = &e.source {
            if source.guid != crate::NIL_WOW_GUID {
                let matches_pet = pet_guid.as_ref().unwrap_or(&String::from(crate::NIL_WOW_GUID)) == &source.guid;
                sql.push(format!("(
                    '{view_id}',
                    '{unit_guid}',
                    {unit_name},
                    {owner_guid},
                    {flags},
                    FALSE
                )",
                    view_id=&e.view_id,
                    unit_guid=&source.guid,
                    unit_name=&crate::sql_format_string(&source.name),
                    owner_guid=crate::sql_format_option_string(if matches_pet { &owner_guid } else { &None }),
                    flags=source.flags,

                ));
                sql.push(String::from(","));
                added += 1;
            }
        } 

        if let Some(dest) = &e.dest {
            if dest.guid != crate::NIL_WOW_GUID {
                let matches_pet = pet_guid.as_ref().unwrap_or(&String::from(crate::NIL_WOW_GUID)) == &dest.guid;
                sql.push(format!("(
                    '{view_id}',
                    '{unit_guid}',
                    {unit_name},
                    {owner_guid},
                    {flags},
                    FALSE
                )",
                    view_id=&e.view_id,
                    unit_guid=&dest.guid,
                    unit_name=&crate::sql_format_string(&dest.name),
                    owner_guid=crate::sql_format_option_string(if matches_pet { &owner_guid } else { &None }),
                    flags=dest.flags,

                ));
                sql.push(String::from(","));
                added += 1;
            }
        }
        
        if let WoWCombatLogEventType::CombatantInfo{guid, ..} = &e.event {
            sql.push(format!("(
                '{view_id}',
                '{unit_guid}',
                NULL,
                NULL,
                0,
                TRUE
            )",
                view_id=&e.view_id,
                unit_guid=&guid,
            ));
            sql.push(String::from(","));
            added += 1;
        }
    }

    if added == 0 {
        return Ok(());
    }

    sql.truncate(sql.len() - 1);
    // We use ON CONFLICT DO NOTHING to ignore things that alreazdy exist.
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(tx).await?;

    // We could probably save some cycles by keeping track of which items were inserted so that
    // we can do less work in the update function, but I think that's probably an over-optimization
    // and not needed (at least at the current moment in time).
    Ok(())
}

// The returned hashmap is keyed by (View UUID, Character GUID). Its value is the character_id in our "wow_match_view_character_presence" table.
type WowCharacterIdMap = HashMap<(Uuid, String), i64>;

async fn update_wow_character_presence_for_events(tx: &mut Transaction<'_, Postgres>, events: &[WoWCombatLogEvent]) -> Result<WowCharacterIdMap, SquadOvError> {
    // We need two sets of SQL statements here - one for the combatant info events
    // and one for the everything else. We need a separate one for combatant info
    // events because otherwise we'll be overwriting valid unit names and guids and flags
    // since the ONLY thing the combatant info stuff needs to do is set the flag to true.
    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        UPDATE squadov.wow_match_view_character_presence AS wcp
        SET unit_name = sub.unit_name,
            owner_guid = sub.owner_guid,
            flags = sub.flags
        FROM ( VALUES
    "));

    // The entries in the "wow_match_view_character_presence" table that
    // need to have has_combatant_info set to true (view id, unit guid).
    let mut bulk_combatants: HashSet<(Uuid, String)> = HashSet::new();

    let mut added = 0;
    for e in events {
        // It'd be nice to share some of this code with the insert functiona bove.
        let owner_guid;
        let pet_guid;
        
        if let Some(advanced) = &e.advanced {
            if advanced.owner_guid != crate::NIL_WOW_GUID && advanced.unit_guid != crate::NIL_WOW_GUID {
                owner_guid = Some(advanced.owner_guid.clone());
                pet_guid = Some(advanced.unit_guid.clone());
            } else {
                owner_guid = None;
                pet_guid = None;
            }
        } else {
            owner_guid = None;
            pet_guid = None;
        };

        // We need to do an update on combatant info here so that the RETURNING statement returns the combatant info too.
        // The combatant info should always come first if it exists so we won't be overriding any important flags here.
        if let WoWCombatLogEventType::CombatantInfo{guid, ..} = &e.event {
            bulk_combatants.insert((e.view_id.clone(), guid.clone()));
        }

        if let Some(source) = &e.source {
            if source.guid != crate::NIL_WOW_GUID {
                let matches_pet = pet_guid.as_ref().unwrap_or(&String::from(crate::NIL_WOW_GUID)) == &source.guid;
                sql.push(format!("(
                    '{view_id}',
                    '{unit_guid}',
                    {unit_name},
                    {owner_guid},
                    {flags}
                )",
                    view_id=&e.view_id,
                    unit_guid=&source.guid,
                    unit_name=&crate::sql_format_string(&source.name),
                    owner_guid=crate::sql_format_option_string(if matches_pet { &owner_guid } else { &None }),
                    flags=source.flags,
                ));
                sql.push(String::from(","));
                added += 1;
            }
        }

        if let Some(dest) = &e.dest {
            if dest.guid != crate::NIL_WOW_GUID {
                let matches_pet = pet_guid.as_ref().unwrap_or(&String::from(crate::NIL_WOW_GUID)) == &dest.guid;
                sql.push(format!("(
                    '{view_id}',
                    '{unit_guid}',
                    {unit_name},
                    {owner_guid},
                    {flags}
                )",
                    view_id=&e.view_id,
                    unit_guid=&dest.guid,
                    unit_name=&crate::sql_format_string(&dest.name),
                    owner_guid=crate::sql_format_option_string(if matches_pet { &owner_guid } else { &None }),
                    flags=dest.flags,
                ));
                sql.push(String::from(","));
                added += 1;
            }
        }
    }

    let mut ret_map: WowCharacterIdMap = HashMap::new();

    if !bulk_combatants.is_empty() {
        let mut view_ids: Vec<Uuid> = vec![];
        let mut unit_guids: Vec<String> = vec![];

        bulk_combatants.into_iter().for_each(|(view, unit)| {
            view_ids.push(view);
            unit_guids.push(unit);
        });

        sqlx::query!(
            "
            UPDATE squadov.wow_match_view_character_presence AS wcp
            SET has_combatant_info = TRUE
            FROM UNNEST($1::UUID[], $2::VARCHAR[]) AS sub(view_id, unit_guid)
            WHERE wcp.view_id = sub.view_id AND wcp.unit_guid = sub.unit_guid
            RETURNING wcp.view_id, wcp.unit_guid, wcp.character_id
            ",
            &view_ids,
            &unit_guids,
        )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .for_each(|x| {
                ret_map.insert((x.view_id, x.unit_guid), x.character_id);
            });
    }

    if added > 0 {
        sql.truncate(sql.len() - 1);
        sql.push(String::from("
            ) AS sub(view_id, unit_guid, unit_name, owner_guid, flags)
            WHERE wcp.view_id = (sub.view_id)::UUID
                AND wcp.unit_guid = sub.unit_guid
            RETURNING wcp.view_id, wcp.unit_guid, wcp.character_id
        "));
        
        let tmp_map = sqlx::query(&sql.join(""))
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|x| {
                Ok(((x.try_get("view_id")?, x.try_get("unit_guid")?), x.try_get("character_id")?))
            })
            .collect::<Result<WowCharacterIdMap, SquadOvError>>()?;

        for ((view_id, unit_guid), character_id) in tmp_map {
            ret_map.insert((view_id, unit_guid), character_id);
        }
    }

    Ok(ret_map)
}


// (View UUID, Log Line) -> Event Id
// Note that we need to do this instead of just returning a vector and mapping 1-1 with the events vector because
// *technically* the database doesn't guarantee any ordering of the INSERT and the resulting RETURNING so I don't
// want to risk relying on it. Instead we know for certain that a given (View UUID, Log Line) is unique so we can
// just rely on the caller-provided unique ID instead.
type WowEventIdMap = HashMap<(i64, i64), i64>;

// Returns the "event_id" for each input event.
async fn create_wow_events(tx: &mut Transaction<'_, Postgres>, events: &[WoWCombatLogEvent], mapping: &WowCharacterIdMap) -> Result<WowEventIdMap, SquadOvError> {
    if events.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_events (
            view_id,
            log_line,
            source_char,
            dest_char,
            tm
        )
        VALUES
    "));

    for e in events {
        let source_tuple = if let Some(source) = &e.source {
            Some((e.view_id.clone(), source.guid.clone()))
        } else {
            None
        };
        let dest_tuple = if let Some(dest) = &e.dest {
            Some((e.view_id.clone(), dest.guid.clone()))
        } else {
            None
        };

        sql.push(format!("(
            {view_id},
            {log_line},
            {source_char},
            {dest_char},
            {tm}
        )",
            view_id=e.alt_view_id,
            log_line=e.log_line,
            source_char=crate::sql_format_option_value(&if let Some(tup) = source_tuple { mapping.get(&tup).copied() } else { None }),
            dest_char=crate::sql_format_option_value(&if let Some(tup) = dest_tuple { mapping.get(&tup).copied() } else { None }),
            tm=crate::sql_format_time(&e.timestamp)
        ));
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sql.push(String::from(" RETURNING view_id, log_line, event_id"));

    Ok(
        sqlx::query(&sql.join(""))
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|x| {
                Ok((
                    (x.try_get("view_id")?, x.try_get("log_line")?),
                    x.try_get("event_id")?)
                )
            })
            .collect::<Result<WowEventIdMap, SquadOvError>>()?
    )
}

async fn bulk_insert_wow_combatant_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap, mapping: &WowCharacterIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut event_sql: Vec<String> = Vec::new();
    event_sql.push(String::from("
        INSERT INTO squadov.wow_match_view_combatants (
            event_id,
            character_id,
            team,
            spec_id,
            rating
        )
        VALUES
    "));

    let mut items_sql: Vec<String> = Vec::new();
    items_sql.push(String::from("
        INSERT INTO squadov.wow_match_view_combatant_items (
            event_id,
            character_id,
            idx,
            item_id,
            ilvl
        )
        VALUES
    "));
    let mut has_items = false;

    let mut talents_sql: Vec<String> = Vec::new();
    talents_sql.push(String::from("
        INSERT INTO squadov.wow_match_view_combatant_talents (
            event_id,
            character_id,
            talent_id,
            is_pvp
        )
        VALUES
    "));
    let mut has_talents = false;

    let mut covenant_sql: Vec<String> = Vec::new();
    covenant_sql.push(String::from("
        INSERT INTO squadov.wow_match_view_combatant_covenants (
            event_id,
            character_id,
            covenant_id,
            soulbind_id,
            soulbind_traits,
            conduit_item_ids,
            conduit_item_ilvls
        )
        VALUES
    "));
    let mut has_covenant = false;

    for e in events {
        if let WoWCombatLogEventType::CombatantInfo{guid, team, spec_id, items, rating, talents, pvp_talents, covenant, ..}= e.event {
            let char_key = (e.view_id.clone(), guid.clone());
            let event_key = (e.alt_view_id, e.log_line);
            let character_id = mapping.get(&char_key).ok_or(SquadOvError::InternalError(format!("COMBATANT: Failed to find char key: {:?}", &char_key)))?;
            let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("COMBATANT: Failed to find event key: {:?}", &event_key)))?;

            event_sql.push(format!("
                (
                    {event_id},
                    {character_id},
                    {team},
                    {spec_id},
                    {rating}
                )
            ",
                event_id=event_id,
                character_id=character_id,
                team=team,
                spec_id=spec_id,
                rating=rating,
            ));
            event_sql.push(String::from(","));

            for (idx, item) in items.iter().enumerate() {
                items_sql.push(format!("
                    (
                        {event_id},
                        {character_id},
                        {idx},
                        {item_id},
                        {ilvl}
                    )
                ",
                    event_id=event_id,
                    character_id=character_id,
                    idx=idx,
                    item_id=item.item_id,
                    ilvl=item.ilvl,
                ));
                items_sql.push(String::from(","));
                has_items = true;
            }

            talents.iter().for_each(|tid| {
                talents_sql.push(format!("
                    (
                        {event_id},
                        {character_id},
                        {talent_id},
                        FALSE
                    )
                ",
                    event_id=event_id,
                    character_id=character_id,
                    talent_id=tid,
                ));
                talents_sql.push(String::from(","));
                has_talents = true;
            });

            pvp_talents.iter().for_each(|tid| {
                talents_sql.push(format!("
                    (
                        {event_id},
                        {character_id},
                        {talent_id},
                        TRUE
                    )
                ",
                    event_id=event_id,
                    character_id=character_id,
                    talent_id=tid,
                ));
                talents_sql.push(String::from(","));
                has_talents = true;
            });

            if let Some(cov) = covenant {
                covenant_sql.push(format!("
                    (
                        {event_id},
                        {character_id},
                        {covenant_id},
                        {soulbind_id},
                        {soulbind_traits},
                        {conduit_item_ids},
                        {conduit_item_ilvls}
                    )
                ",
                    event_id=event_id,
                    character_id=character_id,
                    covenant_id=cov.covenant_id,
                    soulbind_id=cov.soulbind_id,
                    soulbind_traits=crate::sql_format_integer_array(&cov.soulbind_traits),
                    conduit_item_ids=crate::sql_format_bigint_array(&cov.conduits.iter().map(|x| {
                        x.item_id
                    }).collect::<Vec<i64>>()),
                    conduit_item_ilvls=crate::sql_format_integer_array(&cov.conduits.iter().map(|x| {
                        x.ilvl
                    }).collect::<Vec<i32>>()),
                ));
                covenant_sql.push(String::from(","));
                has_covenant = true;
            }
        } else {
            return Err(SquadOvError::BadRequest);
        }
    }

    event_sql.truncate(event_sql.len() - 1);
    event_sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&event_sql.join("")).execute(&mut *tx).await?;

    if has_items {
        items_sql.truncate(items_sql.len() - 1);
        items_sql.push(String::from(" ON CONFLICT DO NOTHING"));
        sqlx::query(&items_sql.join("")).execute(&mut *tx).await?;
    }

    if has_talents {
        talents_sql.truncate(talents_sql.len() - 1);
        talents_sql.push(String::from(" ON CONFLICT DO NOTHING"));
        sqlx::query(&talents_sql.join("")).execute(&mut *tx).await?;
    }

    if has_covenant {
        covenant_sql.truncate(covenant_sql.len() - 1);
        covenant_sql.push(String::from(" ON CONFLICT DO NOTHING"));
        sqlx::query(&covenant_sql.join("")).execute(&mut *tx).await?;
    }

    Ok(())
}

type WowUserCharacterCacheUpdate = HashMap<(i64, String), Uuid>;
async fn bulk_update_wow_user_character_cache(tx: &mut Transaction<'_, Postgres>, update: WowUserCharacterCacheUpdate) -> Result<(), SquadOvError> {
    if update.is_empty() {
        return Ok(())
    }

    let mut user_ids: Vec<i64> = vec![];
    let mut unit_guids: Vec<String> = vec![];
    let mut view_ids: Vec<Uuid> = vec![];
    for ((user_id, unit_guid), view_id) in update {
        user_ids.push(user_id);
        unit_guids.push(unit_guid);
        view_ids.push(view_id);
    }

    sqlx::query!(
        "
            INSERT INTO squadov.wow_user_character_cache (
                user_id,
                unit_guid,
                unit_name,
                spec_id,
                class_id,
                items,
                cache_time,
                build_version
            )
            SELECT DISTINCT ON (data.user_id, data.unit_guid)
                data.user_id,
                data.unit_guid,
                wcp.unit_name,
                wvc.spec_id,
                wvc.class_id,
                ARRAY_AGG(wci.ilvl ORDER BY wci.idx ASC),
                NOW(),
                wmv.build_version
            FROM UNNEST($1::BIGINT[], $2::VARCHAR[], $3::UUID[]) AS data(user_id, unit_guid, view_id)
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.view_id = data.view_id
                    AND wcp.unit_guid = data.unit_guid
            INNER JOIN squadov.wow_match_view_combatants AS wvc
                ON wvc.character_id = wcp.character_id
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.id = data.view_id
            LEFT JOIN squadov.wow_match_view_combatant_items AS wci
                ON wci.event_id = wvc.event_id
            GROUP BY data.user_id, data.unit_guid, wcp.unit_name, wvc.spec_id, wvc.class_id, wmv.build_version, wvc.event_id
            ORDER BY data.user_id, data.unit_guid, wvc.event_id DESC
            ON CONFLICT (user_id, unit_guid) DO UPDATE SET
                unit_name = EXCLUDED.unit_name,
                spec_id = EXCLUDED.spec_id,
                class_id = EXCLUDED.class_id,
                items = EXCLUDED.items,
                cache_time = EXCLUDED.cache_time,
                build_version = EXCLUDED.build_version
        ",
        &user_ids,
        &unit_guids,
        &view_ids,
    )
        .execute(&mut *tx)
        .await?;
    Ok(())
}

async fn bulk_insert_wow_damage_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_damage_events (
            event_id,
            spell_id,
            amount,
            overkill
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("DAMAGE: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::DamageDone{damage, amount, overkill} = x.event {
            sql.push(format!("(
                {event_id},
                {spell_id},
                {amount},
                {overkill}
            )",
                event_id=event_id,
                spell_id=crate::sql_format_option_value(&if let WoWDamageType::SpellDamage(spell) = damage {
                    Some(spell.id)
                } else {
                    None
                }),
                amount=amount,
                overkill=overkill,
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_healing_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_healing_events (
            event_id,
            spell_id,
            amount,
            overheal,
            absorbed
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("HEALING: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::Healing{spell, amount, overheal, absorbed} = x.event {
            sql.push(format!("(
                {event_id},
                {spell_id},
                {amount},
                {overheal},
                {absorbed}
            )",
                event_id=event_id,
                spell_id=spell.id,
                amount=amount,
                overheal=overheal,
                absorbed=absorbed,
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_auras_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_aura_events (
            event_id,
            spell_id,
            aura_type,
            applied
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("AURAS: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::SpellAura{spell, aura_type, applied} = x.event {
            sql.push(format!("(
                {event_id},
                {spell_id},
                '{aura_type}',
                {applied}
            )",
                event_id=event_id,
                spell_id=spell.id,
                aura_type=aura_type.to_string(),
                applied=crate::sql_format_bool(applied),
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));

    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_summon_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_summon_events (
            event_id,
            spell_id
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("SUMMON: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::SpellSummon(spell) = &x.event {
            sql.push(format!("(
                {event_id},
                {spell_id}
            )",
                event_id=event_id,
                spell_id=spell.id,
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));

    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_resurrect_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_resurrect_events (
            event_id,
            spell_id
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("RESURRECT: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::Resurrect(spell) = &x.event {
            sql.push(format!("(
                {event_id},
                {spell_id}
            )",
                event_id=event_id,
                spell_id=spell.id,
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));

    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_subencounter_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_subencounter_events (
            event_id,
            encounter_id,
            encounter_name,
            is_start
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        let event_id = ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("SUBENCOUNTER: Failed to get event key {:?}", &event_key)))?;
        if let WoWCombatLogEventType::EncounterStart{encounter_id, encounter_name, ..} = &x.event {
            sql.push(format!("(
                {event_id},
                {encounter_id},
                {encounter_name},
                TRUE
            )",
                event_id=event_id,
                encounter_id=encounter_id,
                encounter_name=&crate::sql_format_string(&encounter_name),
            ));
        } else if let WoWCombatLogEventType::EncounterEnd{encounter_id, encounter_name, ..} = &x.event {
            sql.push(format!("(
                {event_id},
                {encounter_id},
                {encounter_name},
                FALSE
            )",
                event_id=event_id,
                encounter_id=encounter_id,
                encounter_name=&crate::sql_format_string(encounter_name),
            ));
        } else {
            return Err(SquadOvError::BadRequest);
        }
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));

    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_death_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_death_events (
            event_id
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        sql.push(format!("(
            {event_id}
        )",
            event_id=ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("DEATH: Failed to get event key {:?}", &event_key)))?,
        ));
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    
    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_aura_break_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_aura_break_events (
            event_id,
            aura_spell_id,
            aura_type,
            removed_by_spell_id
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        if let WoWCombatLogEventType::AuraBreak{aura, spell, aura_type} = &x.event {
            sql.push(format!("(
                {event_id},
                {aura_spell_id},
                '{aura_type}',
                {removed_by_spell_id}
            )",
                event_id=ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("AURA BREAK: Failed to get event key {:?}", &event_key)))?,
                aura_spell_id=aura.id,
                aura_type=aura_type.to_string(),
                removed_by_spell_id=crate::sql_format_option_value(&spell.as_ref().map(|x| { x.id })),
            ));
            sql.push(String::from(","));
        }
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    
    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_insert_wow_spell_cast_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql: Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.wow_match_view_spell_cast_events (
            event_id,
            spell_id,
            spell_school,
            is_start,
            is_finish,
            success
        )
        VALUES
    "));

    for x in events {
        let event_key = (x.alt_view_id, x.log_line);
        if let WoWCombatLogEventType::SpellCast{spell, start, finish, success} = &x.event {
            sql.push(format!("(
                {event_id},
                {spell_id},
                {spell_school},
                {is_start},
                {is_finish},
                {success}
            )",
                event_id=ids.get(&event_key).ok_or(SquadOvError::InternalError(format!("AURA BREAK: Failed to get event key {:?}", &event_key)))?,
                spell_id=spell.id,
                spell_school=spell.school,
                is_start=crate::sql_format_bool(*start),
                is_finish=crate::sql_format_bool(*finish),
                success=crate::sql_format_bool(*success),
            ));
            sql.push(String::from(","));
        }
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    
    sqlx::query(&sql.join("")).execute(tx).await?;
    Ok(())
}

async fn bulk_update_combatant_class_from_spell_cast(tx: &mut Transaction<'_, Postgres>, events: &[WoWCombatLogEvent]) -> Result<(), SquadOvError> {
    // What needs to happen here is that we need to update the class_id column in wow_match_view_combatants 
    // by inferring what class it is from the spell ID. Really just a lot of fun all around.
    let mut player_guids: Vec<String> = vec![];
    let mut spell_ids: Vec<i64> = vec![];
    let mut alt_view_ids: Vec<i64> = vec![];
    
    events.iter().for_each(|x| {
        if let WoWCombatLogEventType::ClassUpdateFromSpell(update) = &x.event {
            player_guids.push(update.player_guid.clone());
            spell_ids.push(update.spell_id);
            alt_view_ids.push(x.alt_view_id);
        }
    });

    sqlx::query!(
        "
        UPDATE squadov.wow_match_view_combatants AS wvc
        SET class_id = sub.class_id
        FROM (
            SELECT wve.event_id, wsc.class_id
            FROM UNNEST($1::VARCHAR[], $2::BIGINT[], $3::BIGINT[]) AS t(guid, spell_id, view_id)
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.alt_id = t.view_id
            INNER JOIN squadov.wow_spell_to_class AS wsc
                ON wsc.spell_id = t.spell_id
                    AND wmv.build_version LIKE wsc.build_id  || '.%'
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.view_id = wmv.alt_id
            INNER JOIN squadov.wow_match_view_combatants AS wvc
                ON wvc.event_id = wve.event_id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.character_id = wvc.character_id
                    AND wcp.view_id = wmv.id
                    AND wcp.unit_guid = t.guid
        ) AS sub
        WHERE wvc.event_id = sub.event_id
        ",
        &player_guids,
        &spell_ids,
        &alt_view_ids,
    )
        .execute(&mut *tx)
        .await?;

    // This is maybe a bit redundant but this information needs to be stored in
    // wow_match_view_combatants as well as wow_match_view_character_presence. We
    // later decided to add the identical column to wow_match_view_character_presence
    // because certain modes (RBGs, WoW classic arena) will not have COMBATANT_INFO
    // so no wow_match_view_combatants rows will exist but we still need to store the
    // class information elsewhere.
    sqlx::query!(
        r#"
        UPDATE squadov.wow_match_view_character_presence AS wcp
        SET class_id = sub.class_id
        FROM (
            SELECT t.guid, wsc.class_id, wmv.id AS "view_id"
            FROM UNNEST($1::VARCHAR[], $2::BIGINT[], $3::BIGINT[]) AS t(guid, spell_id, view_id)
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.alt_id = t.view_id
            INNER JOIN squadov.wow_spell_to_class AS wsc
                ON wsc.spell_id = t.spell_id
                    AND wmv.build_version LIKE wsc.build_id  || '.%'
        ) AS sub
        WHERE wcp.view_id = sub.view_id
            AND unit_guid = sub.guid
        "#,
        &player_guids,
        &spell_ids,
        &alt_view_ids,
    )
        .execute(&mut *tx)
        .await?;
    Ok(())
}

async fn bulk_update_user_character_cache_from_spell_cast(tx: &mut Transaction<'_, Postgres>, events: &[WoWCombatLogEvent]) -> Result<(), SquadOvError> {
    let mut player_set: HashSet<(i64, String)> = HashSet::new();

    let mut player_guids: Vec<String> = vec![];
    let mut player_names: Vec<String> = vec![];
    let mut player_user_ids: Vec<i64> = vec![];
    let mut spell_ids: Vec<i64> = vec![];
    let mut player_build_versions: Vec<String> = vec![];

    events.iter().for_each(|x| {
        if let WoWCombatLogEventType::ClassUpdateFromSpell(update) = &x.event {
            if let Some(user_id) = update.user_id {
                if !player_set.contains(&(user_id, update.player_guid.clone())) {
                    player_guids.push(update.player_guid.clone());
                    player_names.push(update.player_name.clone());
                    player_user_ids.push(user_id);
                    player_build_versions.push(update.build_version.clone());
                    spell_ids.push(update.spell_id);
                    player_set.insert((user_id, update.player_guid.clone()));
                }
            }
        }
    });

    if player_guids.is_empty() {
        return Ok(());
    }

    sqlx::query!(
        "
        INSERT INTO squadov.wow_user_character_cache (
            user_id,
            unit_guid,
            unit_name,
            class_id,
            cache_time,
            build_version
        )
        SELECT t.user_id, t.unit_guid, t.unit_name, wsc.class_id, NOW(), t.build_version
        FROM UNNEST($1::BIGINT[], $2::VARCHAR[], $3::VARCHAR[], $4::VARCHAR[], $5::BIGINT[]) AS t(user_id, unit_guid, unit_name, build_version, spell_id)
        INNER JOIN squadov.wow_spell_to_class AS wsc
            ON wsc.spell_id = t.spell_id
                AND t.build_version LIKE wsc.build_id  || '.%'
        ON CONFLICT (user_id, unit_guid) DO UPDATE SET
            unit_name = EXCLUDED.unit_name,
            class_id = EXCLUDED.class_id,
            cache_time = EXCLUDED.cache_time,
            build_version = EXCLUDED.build_version
        ",
        &player_user_ids,
        &player_guids,
        &player_names,
        &player_build_versions,
        &spell_ids,
    )
        .execute(tx)
        .await?;
    Ok(())
}

async fn bulk_insert_wow_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>, ids: &WowEventIdMap, mapping: &WowCharacterIdMap) -> Result<(), SquadOvError> {
    // We first split the input events into individual vectors that are split according to how our database tables are split.
    // This way we can do a O(N) operation to parse out the bulk inserts instead of a M O(N) operation (where M is the number of tables we have).
    // M is just a constant factor but I'd rather avoid it. Note that we also want to update the "wow_user_character_cache" table here if we 
    // encounter events that have the appropriate combat log flag set.
    let mut combatant_events: Vec<WoWCombatLogEvent> = vec![];
    let mut damage_events: Vec<WoWCombatLogEvent> = vec![];
    let mut healing_events: Vec<WoWCombatLogEvent> = vec![];
    let mut auras_events: Vec<WoWCombatLogEvent> = vec![];
    let mut summon_events: Vec<WoWCombatLogEvent> = vec![];
    let mut resurrect_events: Vec<WoWCombatLogEvent> = vec![];
    let mut subencounter_events: Vec<WoWCombatLogEvent> = vec![];
    let mut death_events: Vec<WoWCombatLogEvent> = vec![];
    let mut aura_break_events: Vec<WoWCombatLogEvent> = vec![];
    let mut spell_cast_events: Vec<WoWCombatLogEvent> = vec![];
    let mut class_updates_from_spell_events: Vec<WoWCombatLogEvent> = vec![];

    // (User ID, Unit GUID) -> View UUID. This way we are able to do an upsert
    // via an INSERT/SELECT to find the combatant info in the view for this unit.
    let mut user_character_cache: HashMap<(i64, String), Uuid> = HashMap::new();

    events.into_iter().for_each(|x| {
        if let Some(source) = &x.source {
            if source.flags & COMBATLOG_FILTER_ME == COMBATLOG_FILTER_ME && source.guid != crate::NIL_WOW_GUID {
                user_character_cache.insert((x.user_id, source.guid.clone()), x.view_id.clone());
            }
        }

        if let Some(dest) = &x.dest {
            if dest.flags & COMBATLOG_FILTER_ME == COMBATLOG_FILTER_ME && dest.guid != crate::NIL_WOW_GUID {
                user_character_cache.insert((x.user_id, dest.guid.clone()), x.view_id.clone());
            }
        }

        match x.event {
            WoWCombatLogEventType::CombatantInfo{..} => combatant_events.push(x),
            WoWCombatLogEventType::DamageDone{..} => damage_events.push(x),
            WoWCombatLogEventType::Healing{..} => healing_events.push(x),
            WoWCombatLogEventType::SpellAura{..} => auras_events.push(x),
            WoWCombatLogEventType::SpellSummon(..) => summon_events.push(x),
            WoWCombatLogEventType::Resurrect(..) => resurrect_events.push(x),
            WoWCombatLogEventType::EncounterStart{..} | WoWCombatLogEventType::EncounterEnd{..} => subencounter_events.push(x),
            WoWCombatLogEventType::UnitDied{..} => death_events.push(x),
            WoWCombatLogEventType::AuraBreak{..} => aura_break_events.push(x),
            WoWCombatLogEventType::SpellCast{..} => spell_cast_events.push(x),
            WoWCombatLogEventType::ClassUpdateFromSpell{..} => class_updates_from_spell_events.push(x),
            _ => log::warn!("Handling an event that can't be parsed into a table? {:?}", x),
        }
    });

    bulk_insert_wow_combatant_events(&mut *tx, combatant_events, ids, mapping).await?;
    bulk_insert_wow_damage_events(&mut *tx, damage_events, ids).await?;
    bulk_insert_wow_healing_events(&mut *tx, healing_events, ids).await?;
    bulk_insert_wow_auras_events(&mut *tx, auras_events, ids).await?;
    bulk_insert_wow_summon_events(&mut *tx, summon_events, ids).await?;
    bulk_insert_wow_resurrect_events(&mut *tx, resurrect_events, ids).await?;
    bulk_insert_wow_subencounter_events(&mut *tx, subencounter_events, ids).await?;
    bulk_insert_wow_death_events(&mut *tx, death_events, ids).await?;
    bulk_insert_wow_aura_break_events(&mut *tx, aura_break_events, ids).await?;
    bulk_insert_wow_spell_cast_events(&mut *tx, spell_cast_events, ids).await?;
    bulk_update_combatant_class_from_spell_cast(&mut *tx, &class_updates_from_spell_events).await?;
    bulk_update_wow_user_character_cache(&mut *tx, user_character_cache).await?;
    bulk_update_user_character_cache_from_spell_cast(&mut *tx, &class_updates_from_spell_events).await?;

    Ok(())
}

pub async fn store_wow_combat_log_events(tx: &mut Transaction<'_, Postgres>, events: Vec<WoWCombatLogEvent>) -> Result<(), SquadOvError> {
    // First we filter out events we don't need. We're parsing more events than we care to store at the moment.
    // That is NOT an error in parsing as some events are crucial for our Kafka logic such as knowing when to
    // store an updated offset/commit events.
    let events: Vec<_> = events.into_iter().filter(|x| {
        match &x.event {
            WoWCombatLogEventType::ArenaStart{..} | 
                WoWCombatLogEventType::ArenaEnd{..} | 
                WoWCombatLogEventType::ChallengeModeStart{..} | 
                WoWCombatLogEventType::ChallengeModeEnd{..} | 
                WoWCombatLogEventType::Unknown => false,
            WoWCombatLogEventType::UnitDied{unconcious} => !unconcious,
            _ => true,
        }
    }).collect();

    if events.is_empty() {
        return Ok(());
    }

    // For every event that comes through, we need to add an entry to the "wow_match_view_events" table.
    // If the event has a "source" or "dest" field:
    //  1) Insert or update an entry in the "wow_match_view_character_presence" table.
    // If the event is COMBATANT_INFO:
    //  1) Insert or update an entry in the "wow_match_view_character_presence" table (with NULL name, flags, and owner guid).
    //  2) Create a new event in the "wow_match_view_events" table.
    //  3) Insert combatant info into the "wow_match_view_combatants" and "wow_match_view_combatant_items" tables.
    //  4) Link the new combatant info into the "wow_user_character_cache" if the character belongs to the owner of the combat log.
    // For all other events:
    //  1) Create a new event in the "wow_match_view_events" table.
    //  2) Create a new event in the corresponding event table:
    //         - wow_match_view_damage_events
    //         - wow_match_view_healing_events
    //         - wow_match_view_aura_events
    //         - wow_match_view_summon_events
    //         - wow_match_view_resurrect_events
    //         - wow_match_view_subencounter_events
    //         - wow_match_view_death_events
    // We want to try and be efficient about this by sending multi-row inserts as much as possible.
    // Thus, we use this order of operations:
    //  1) Perform inserts into "wow_match_view_character_presence" for only the characters that are missing (source/dest/combatant info).
    //     We need to limit this to missing characters because it's a possibility that we can have multiple
    //     events that reference the same character. Thus, any sort of bulk upsert would fail.
    insert_missing_wow_character_presence_for_events(&mut *tx, &events).await?;

    //  2) Perform updates on "wow_match_view_character_presence". Now that we're guaranteed that every event's source/dest
    //     is in the "wow_match_view_character_presence" table, we can perform a regular UPDATE (instead of an INSERT) and thus
    //     multiple events can touch the same row no problem.
    //
    //     Data that we'd want to update are:
    //          1) Flags
    //          2) Owner GUID
    //          3) Unit Name
    let character_id_map = update_wow_character_presence_for_events(&mut *tx, &events).await?;

    //  3) Bullk add into the "wow_match_view_events". We are able to obtain proper values for source_char and dest_char
    //     from the previous two operations.
    let event_ids = create_wow_events(&mut *tx, &events, &character_id_map).await?;

    //  4) AT THIS POINT THE LOGIC MUST SPLIT DEPENDING ON THE TYPE OF THE INCOMING EVENT.
    //     a) COMBATANT_INFO: We can pull character_id from steps #1 and #2. So it becomes trivial to
    //                        bulk add into the wow_match_view_combatants and wow_match_view_combatant_items tables.
    //                        
    //                        It's also when processing the COMBATANT_INFO can we also finally update the "wow_match_view_character_presence"
    //                        table to know that the combatant info exists.
    //     b) All other events are trivially bulk insertable - the only thing that changes between them is the table/data that we insert into.
    bulk_insert_wow_events(&mut *tx, events, &event_ids, &character_id_map).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate env_logger;

    fn init() {
        std::env::set_var("RUST_LOG", "debug");
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_is_finish_token() {
        let t1 = RawWoWCombatLogPayload{
            timestamp: Utc::now(),
            parts: vec![String::from("Test"), String::from("1")],
            raw_log: None,
            log_line: 1,
            version: 2,
        };
        assert_eq!(t1.is_finish_token(), false);

        let t2 = RawWoWCombatLogPayload{
            timestamp: Utc::now(),
            parts: vec![String::from("SQUADOV_END_COMBAT_LOG")],
            raw_log: None,
            log_line: 1,
            version: 2,
        };
        assert_eq!(t2.is_finish_token(), true);
    }

    #[test]
    fn test_split_wow_combat_log_tokens() {
        init();

        struct TestDatum {
            input: &'static str,
            output: Vec<&'static str>,
        }

        let test_data = vec![
            TestDatum{
                input: r#"CHALLENGE_MODE_START,"Plaguefall",2289,379,2,[9]"#,
                output: vec![
                    "CHALLENGE_MODE_START",
                    "Plaguefall",
                    "2289",
                    "379",
                    "2",
                    "[9]"
                ],
            },
            TestDatum{
                input: r#"COMBATANT_INFO,Player-3685-0CF2175A,0,1042,155,1502,448,0,0,0,307,307,307,0,0,518,518,518,0,128,495,495,495,1038,70,(269569,326732,234299,230332,223817,326734,317866),(0,210256,246806,305394),[7,1,[],[(1305),(1312),(1316),(1308),(1310)],[(216,158),(129,184)]],[(178777,184,(),(6807,6652,7194,1498,6646),()),(175885,184,(),(6789,1498,6646),()),(175872,184,(),(6782,1498,6646),()),(0,0,(),(),()),(171412,190,(6230,0,0),(7103,6716,6650,6649,1487),()),(183015,213,(),(7188,6652,7194,1485,6646),()),(178701,184,(),(6807,6652,1498,6646),()),(175856,184,(),(6782,1498,6646),()),(178807,184,(),(6807,43,7193,1498,6646),()),(181661,177,(),(6652,1472,5874,6616),()),(178933,184,(6170,0,0),(6807,6652,7194,1498,6646),()),(178781,184,(),(6807,6652,7194,1498,6646),()),(184052,184,(),(6789,1498,6646),()),(175884,184,(),(6782,1498,6646),()),(180123,184,(6208,0,0),(6807,6652,1498,6646),()),(178866,184,(6229,0,0),(6807,6652,1498,6646),()),(0,0,(),(),()),(0,0,(),(),())],[Player-3685-0A49ECD5,21562,Player-3685-0CF2175A,267344],9,0,0,0"#,
                output: vec![
                    "COMBATANT_INFO",
                    "Player-3685-0CF2175A",
                    "0",
                    "1042",
                    "155",
                    "1502",
                    "448",
                    "0",
                    "0",
                    "0",
                    "307",
                    "307",
                    "307",
                    "0",
                    "0",
                    "518",
                    "518",
                    "518",
                    "0",
                    "128",
                    "495",
                    "495",
                    "495",
                    "1038",
                    "70",
                    "(269569,326732,234299,230332,223817,326734,317866)",
                    "(0,210256,246806,305394)",
                    "[7,1,[],[(1305),(1312),(1316),(1308),(1310)],[(216,158),(129,184)]]",
                    "[(178777,184,(),(6807,6652,7194,1498,6646),()),(175885,184,(),(6789,1498,6646),()),(175872,184,(),(6782,1498,6646),()),(0,0,(),(),()),(171412,190,(6230,0,0),(7103,6716,6650,6649,1487),()),(183015,213,(),(7188,6652,7194,1485,6646),()),(178701,184,(),(6807,6652,1498,6646),()),(175856,184,(),(6782,1498,6646),()),(178807,184,(),(6807,43,7193,1498,6646),()),(181661,177,(),(6652,1472,5874,6616),()),(178933,184,(6170,0,0),(6807,6652,7194,1498,6646),()),(178781,184,(),(6807,6652,7194,1498,6646),()),(184052,184,(),(6789,1498,6646),()),(175884,184,(),(6782,1498,6646),()),(180123,184,(6208,0,0),(6807,6652,1498,6646),()),(178866,184,(6229,0,0),(6807,6652,1498,6646),()),(0,0,(),(),()),(0,0,(),(),())]",
                    "[Player-3685-0A49ECD5,21562,Player-3685-0CF2175A,267344]",
                    "9",
                    "0",
                    "0",
                    "0"
                ]
            },
            TestDatum{
                input: r#"ENCOUNTER_START,2382,"Amarth, The Harvester",23,5,2289"#,
                output: vec![
                    "ENCOUNTER_START",
                    "2382",
                    "Amarth, The Harvester",
                    "23",
                    "5",
                    "2289"
                ]
            }
        ];

        for td in &test_data {
            let tokens = split_wow_combat_log_tokens(td.input);
            assert_eq!(tokens, td.output);
        }
    }
}