use crate::SquadOvError;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::str::FromStr;

const LOG_FLUSH: &'static str = "//SQUADOV_COMBAT_LOG_FLUSH";

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Ff14DotHotType {
    DoT,
    HoT,
    Unknown
}

impl FromStr for Ff14DotHotType {
    type Err = SquadOvError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "dot" => Ff14DotHotType::DoT,
            "hot" => Ff14DotHotType::HoT,
            _ => Ff14DotHotType::Unknown
        })
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="type")]
pub enum Ff14CombatLogEvent {
    // 0
    LogLine,
    // 1
    ChangeZone{
        id: i64,
        name: String,
    },
    // 2
    ChangePrimaryPlayer{
        id: i64,
        name: String,
    },
    // 3
    AddCombatant{
        id: i64,
        name: String,
        job: i64,
        level: i64,
        owner_id: i64,
        world_id: i64,
        world: String,
        npc_name_id: Option<i64>,
        npc_base_id: Option<i64>,
        current_hp: i64,
        hp: i64,
        current_mp: i64,
        mp: i64,
    },
    // 4
    RemoveCombatant{
        id: i64,
    },
    // 11
    PartyList{
        count: i64,
        members: Vec<i64>,
    },
    // 12
    PlayerStats{
        job: i64,
        strength: i64,
        dexterity: i64,
        vitality: i64,
        intelligence: i64,
        mind: i64,
        piety: i64,
        attack_power: i64,
        direct_hit: i64,
        critical_hit: i64,
        attack_magic_potency: i64,
        heal_magic_potency: i64,
        determination: i64,
        skill_speed: i64,
        spell_speed: i64,
        tenacity: i64,
    },
    // 20
    NetworkStartsCasting{
        source_id: i64,
        source: String,
        spell_id: i64,
        spell: String,
        target_id: i64,
        target: String,
        cast_time: f64,
    },
    // 21 and 22
    NetworkAbility{
        source_id: i64,
        source: String,
        spell_id: i64,
        spell: String,
        target_id: i64,
        target: String,
        flags: i64,
        damage: i64,
        target_current_hp: Option<i64>,
        target_hp: Option<i64>,
        target_current_mp: Option<i64>,
        target_mp: Option<i64>,
        current_hp: i64,
        hp: i64,
        current_mp: i64,
        mp: i64,
        sequence: i64,
    },
    // 23
    NetworkCancelAbility{
        source_id: i64,
        source: String,
        spell_id: i64,
        spell: String,
        reason: String,
    },
    // 24
    NetworkDot{
        id: i64,
        name: String,
        which: Ff14DotHotType, // DoT or HoT
        effect_id: i64,
        damage: i64,
        current_hp: i64,
        hp: i64,
        current_mp: i64,
        mp: i64,
    },
    // 25
    NetworkDeath{
        target_id: i64,
        target: String,
        source_id: i64,
        source: String,
    },
    // 26
    NetworkBuff{
        effect_id: i64,
        effect: String,
        duration: f64,
        source_id: i64,
        source: String,
        target_id: i64,
        target: String,
        count: i64,
        target_max_hp: i64,
        source_max_hp: i64,
    },
    // 27
    NetworkTargetIcon,
    // 28
    NetworkRaidMarker,
    // 29
    NetworkTargetMarker,
    // 30
    NetworkBuffRemove{
        effect_id: i64,
        effect: String,
        source_id: i64,
        source: String,
        target_id: i64,
        target: String,
        count: i64,
    },
    // 31
    NetworkGauge{
        id: i64,
        data0: i64,
        data1: i64,
        data2: i64,
        data3: i64,
    },
    // 32
    NetworkWorld,
    // 33
    Network6DActorControl{
        instance: i64,
        command: i64,
        data0: i64,
        data1: i64,
        data2: i64,
        data3: i64,
    },
    // 34
    NetworkNameToggle,
    // 35
    NetworkTether{
        source_id: i64,
        source: String,
        target_id: i64,
        target: String,
        tether_type: i64,
    },
    // 36
    LimitBreak{
        value: i64,
        // Maximum number of bars possible
        bars: i32,
    },
    // 37
    NetworkActionSync{
        id: i64,
        name: String,
        sequence: i64,
        current_hp: i64,
        hp: i64,
        current_mp: i64,
        mp: i64,
    },
    // 38
    NetworkStatusEffects,
    // 39
    NetworkUpdateHp{
        id: i64,
        name: String,
        current_hp: Option<i64>,
        hp: Option<i64>,
        current_mp: Option<i64>,
        mp: Option<i64>,
    },
    // 40
    Map{
        region_id: i64,
        region_name: String,
        place_name: String,
        place_name_sub: String,
    },
    // 41
    SystemLogMessage,
    // 251,
    Debug,
    // 252
    PacketDump,
    // 253
    Version,
    // 254
    Error,
    Unknown,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag="form")]
pub enum Ff14PacketData {
    Raw{inner: String},
    Parsed{inner: Ff14CombatLogEvent},
    Flush,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Ff14CombatLogPacket {
    pub partition_id: String,
    pub time: DateTime<Utc>,
    pub data: Ff14PacketData,
}

fn parse_change_zone(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 2 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::ChangeZone{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
        }
    )
}

fn parse_change_primary_player(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 2 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::ChangePrimaryPlayer{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
        }
    )
}

fn parse_add_combatant(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 13 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::AddCombatant{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
            job: i64::from_str_radix(parts[2], 16)?,
            level: i64::from_str_radix(parts[3], 16)?,
            owner_id: i64::from_str_radix(parts[4], 16)?,
            world_id: i64::from_str_radix(parts[5], 16)?,
            world: String::from(parts[6]),
            npc_name_id: if parts[7].is_empty() {
                None
            } else {
                Some(i64::from_str_radix(parts[7], 16)?)
            },
            npc_base_id: if parts[8].is_empty() {
                None
            } else {
                Some(i64::from_str_radix(parts[8], 16)?)
            },
            current_hp: parts[9].parse()?,
            hp: parts[10].parse()?,
            current_mp: parts[11].parse()?,
            mp: parts[12].parse()?,
        }
    )
}

fn parse_remove_combatant(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 1 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::RemoveCombatant{
            id: i64::from_str_radix(parts[0], 16)?,
        }
    )
}

fn parse_party_list(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 1 {
        return Err(SquadOvError::BadRequest);
    }

    // I'm not actually sure if this value is written as hex or decimal...
    // And when in doubt do both! Presumably the initial parse will fail if it's hex. LMAO.
    let count: i64 = parts[0].parse().or_else(|_| i64::from_str_radix(parts[0], 16))?;
    if parts.len() < (1 + count) as usize {
        return Err(SquadOvError::BadRequest);
    }

    let mut members: Vec<i64> = vec![];
    for i in 0usize..count as usize {
        members.push(
            i64::from_str_radix(parts[1usize+i], 16)?
        );
    }

    Ok(
        Ff14CombatLogEvent::PartyList{
            count,
            members,
        }
    )
}

fn parse_player_stats(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 17 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::PlayerStats{
            job: parts[0].parse()?,
            strength: parts[1].parse()?,
            dexterity: parts[2].parse()?,
            vitality: parts[3].parse()?,
            intelligence: parts[4].parse()?,
            mind: parts[5].parse()?,
            piety: parts[6].parse()?,
            attack_power: parts[7].parse()?,
            direct_hit: parts[8].parse()?,
            critical_hit: parts[9].parse()?,
            attack_magic_potency: parts[10].parse()?,
            heal_magic_potency: parts[11].parse()?,
            determination: parts[12].parse()?,
            skill_speed: parts[13].parse()?,
            spell_speed: parts[14].parse()?,
            tenacity: parts[16].parse()?,
        }
    )
}

fn parse_network_starts_casting(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 7 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkStartsCasting{
            source_id: i64::from_str_radix(parts[0], 16)?,
            source: String::from(parts[1]),
            spell_id: i64::from_str_radix(parts[2], 16)?,
            spell: String::from(parts[3]),
            target_id: i64::from_str_radix(parts[4], 16)?,
            target: String::from(parts[5]),
            cast_time: parts[6].parse()?,
        }
    )
}

fn parse_network_ability(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 43 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkAbility{
            source_id: i64::from_str_radix(parts[0], 16)?,
            source: String::from(parts[1]),
            spell_id: i64::from_str_radix(parts[2], 16)?,
            spell: String::from(parts[3]),
            target_id: i64::from_str_radix(parts[4], 16)?,
            target: String::from(parts[5]),
            flags: i64::from_str_radix(parts[6], 16)?,
            damage: i64::from_str_radix(parts[7], 16)?,
            target_current_hp: if parts[22].is_empty() {
                None
            } else {
                Some(parts[22].parse()?)
            },
            target_hp: if parts[23].is_empty() {
                None
            } else {
                Some(parts[23].parse()?)
            },
            target_current_mp: if parts[24].is_empty() {
                None
            } else {
                Some(parts[24].parse()?)
            },
            target_mp: if parts[25].is_empty() {
                None
            } else {
                Some(parts[25].parse()?)
            },
            current_hp: parts[32].parse()?,
            hp: parts[33].parse()?,
            current_mp: parts[34].parse()?,
            mp: parts[35].parse()?,
            sequence: i64::from_str_radix(parts[42], 16)?,
        }
    )
}

fn parse_network_cancel_ability(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 5 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkCancelAbility{
            source_id: i64::from_str_radix(parts[0], 16)?,
            source: String::from(parts[1]),
            spell_id: i64::from_str_radix(parts[2], 16)?,
            spell: String::from(parts[3]),
            reason: String::from(parts[4]),
        }
    )
}

fn parse_network_dot(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 9 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkDot{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
            which: Ff14DotHotType::from_str(parts[2])?,
            effect_id: i64::from_str_radix(parts[3], 16)?,
            damage: i64::from_str_radix(parts[4], 16)?,
            current_hp: parts[5].parse()?,
            hp: parts[6].parse()?,
            current_mp: parts[7].parse()?,
            mp: parts[8].parse()?,
        }
    )
}

fn parse_network_death(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 4 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkDeath{
            target_id: i64::from_str_radix(parts[0], 16)?,
            target: String::from(parts[1]),
            source_id: i64::from_str_radix(parts[2], 16)?,
            source: String::from(parts[3]),
        }
    )
}

fn parse_network_buff(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 10 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkBuff{
            effect_id: i64::from_str_radix(parts[0], 16)?,
            effect: String::from(parts[1]),
            duration: parts[2].parse()?,
            source_id: i64::from_str_radix(parts[3], 16)?,
            source: String::from(parts[4]),
            target_id: i64::from_str_radix(parts[5], 16)?,
            target: String::from(parts[6]),
            count: i64::from_str_radix(parts[7], 16)?,
            target_max_hp: parts[8].parse()?,
            source_max_hp: parts[9].parse()?,
        }
    )
}

fn parse_network_buff_remove(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 8 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkBuffRemove{
            effect_id: i64::from_str_radix(parts[0], 16)?,
            effect: String::from(parts[1]),
            source_id: i64::from_str_radix(parts[3], 16)?,
            source: String::from(parts[4]),
            target_id: i64::from_str_radix(parts[5], 16)?,
            target: String::from(parts[6]),
            count: i64::from_str_radix(parts[7], 16)?,
        }
    )
}

fn parse_network_gauge(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 5 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkGauge{
            id: i64::from_str_radix(parts[0], 16)?,
            data0: i64::from_str_radix(parts[1], 16)?,
            data1: i64::from_str_radix(parts[2], 16)?,
            data2: i64::from_str_radix(parts[3], 16)?,
            data3: i64::from_str_radix(parts[4], 16)?,
        }
    )
}

fn parse_network_6d_actor_control(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 6 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::Network6DActorControl{
            instance: i64::from_str_radix(parts[0], 16)?,
            command: i64::from_str_radix(parts[1], 16)?,
            data0: i64::from_str_radix(parts[2], 16)?,
            data1: i64::from_str_radix(parts[3], 16)?,
            data2: i64::from_str_radix(parts[4], 16)?,
            data3: i64::from_str_radix(parts[5], 16)?,
        }
    )
}

fn parse_network_tether(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 7 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkTether{
            source_id: i64::from_str_radix(parts[0], 16)?,
            source: String::from(parts[1]),
            target_id: i64::from_str_radix(parts[2], 16)?,
            target: String::from(parts[3]),
            tether_type: i64::from_str_radix(parts[6], 16)?,
        }
    )
}

fn parse_limit_break(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 2 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::LimitBreak{
            value: i64::from_str_radix(parts[0], 16)?,
            bars: parts[1].parse()?,
        }
    )
}

fn parse_network_action_sync(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 7 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkActionSync{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
            sequence: i64::from_str_radix(parts[2], 16)?,
            current_hp: parts[3].parse()?,
            hp: parts[4].parse()?,
            current_mp: parts[5].parse()?,
            mp: parts[6].parse()?,
        }
    )
}

fn parse_network_update_hp(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 6 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::NetworkUpdateHp{
            id: i64::from_str_radix(parts[0], 16)?,
            name: String::from(parts[1]),
            current_hp: if parts[2].is_empty() {
                None
            } else {
                Some(parts[2].parse()?)
            },
            hp: if parts[3].is_empty() {
                None
            } else {
                Some(parts[3].parse()?)
            },
            current_mp: if parts[4].is_empty() {
                None
            } else {
                Some(parts[4].parse()?)
            },
            mp: if parts[5].is_empty() {
                None
            } else {
                Some(parts[5].parse()?)
            },
        }
    )
}

fn parse_map(parts: &[&str]) -> Result<Ff14CombatLogEvent, SquadOvError> {
    if parts.len() < 4 {
        return Err(SquadOvError::BadRequest);
    }

    Ok(
        Ff14CombatLogEvent::Map{
            region_id: i64::from_str_radix(parts[0], 16)?,
            region_name: String::from(parts[1]),
            place_name: String::from(parts[2]),
            place_name_sub: String::from(parts[3]),
        }
    )
}

fn internal_parse_ff14_combat_log_line(partition_id: String, parts: &[&str]) -> Result<Ff14CombatLogPacket, SquadOvError> {
    if parts.is_empty() || parts.len() < 2 {
        return Err(SquadOvError::BadRequest);
    }

    let action_id: i64 = parts[0].parse()?;
    let time: DateTime<Utc> = DateTime::from(DateTime::parse_from_rfc3339(parts[1])?);
    let rest = &parts[2..];
    Ok(
        Ff14CombatLogPacket{
            partition_id,
            time,
            data: Ff14PacketData::Parsed{
                inner: match action_id {
                    0 => Ff14CombatLogEvent::LogLine,
                    1 => parse_change_zone(rest)?,
                    2 => parse_change_primary_player(rest)?,
                    3 => parse_add_combatant(rest)?,
                    4 => parse_remove_combatant(rest)?,
                    11 => parse_party_list(rest)?,
                    12 => parse_player_stats(rest)?,
                    20 => parse_network_starts_casting(rest)?,
                    21 | 22 => parse_network_ability(rest)?,
                    23 => parse_network_cancel_ability(rest)?,
                    24 => parse_network_dot(rest)?,
                    25 => parse_network_death(rest)?,
                    26 => parse_network_buff(rest)?,
                    27 => Ff14CombatLogEvent::NetworkTargetIcon,
                    28 => Ff14CombatLogEvent::NetworkRaidMarker,
                    29 => Ff14CombatLogEvent::NetworkTargetMarker,
                    30 => parse_network_buff_remove(rest)?,
                    31 => parse_network_gauge(rest)?,
                    32 => Ff14CombatLogEvent::NetworkWorld,
                    33 => parse_network_6d_actor_control(rest)?,
                    34 => Ff14CombatLogEvent::NetworkNameToggle,
                    35 => parse_network_tether(rest)?,
                    36 => parse_limit_break(rest)?,
                    37 => parse_network_action_sync(rest)?,
                    38 => Ff14CombatLogEvent::NetworkStatusEffects,
                    39 => parse_network_update_hp(rest)?,
                    40 => parse_map(rest)?,
                    41 => Ff14CombatLogEvent::SystemLogMessage,
                    251 => Ff14CombatLogEvent::Debug,
                    252 => Ff14CombatLogEvent::PacketDump,
                    253 => Ff14CombatLogEvent::Version,
                    254 => Ff14CombatLogEvent::Error,
                    _ => Ff14CombatLogEvent::Unknown,
                },
            },
        }
    )
}

pub fn parse_ff14_combat_log_line(partition_id: String, line: String) -> (String, Result<Ff14CombatLogPacket, SquadOvError>) {
    let parts: Vec<&str> = line.split("|").collect();
    (
        line.clone(),
        if line == LOG_FLUSH {
            Ok(Ff14CombatLogPacket{
                partition_id,
                time: Utc::now(),
                data: Ff14PacketData::Flush,
            })
        } else {
            internal_parse_ff14_combat_log_line(partition_id, &parts)
        },
    )
}