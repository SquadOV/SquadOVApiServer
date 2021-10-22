use crate::{
    SquadOvError,
    games::SquadOvGames,
    matches,
    riot::games::{
        LolMatchDto,
        LolTeamDto,
        LolBanDto,
        LolMatchTimelineInfoDto,
        LolParticipantDto,
        LolMatchParticipantFrameDto,
        LolMatchEventDto,
    }
};
use sqlx::{Transaction, Postgres};
use uuid::Uuid;
use chrono::{DateTime, Utc};

async fn update_lol_match_game_start_time(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, game_start_time: Option<DateTime<Utc>>) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        UPDATE squadov.lol_matches
        SET game_start_time = LEAST($2, game_start_time)
        WHERE match_uuid = $1
        ",
        match_uuid,
        game_start_time
    )
        .execute(ex)
        .await?;
    Ok(())
}

async fn link_match_uuid_to_lol_match(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, platform: &str, game_id: i64, game_start_time: Option<DateTime<Utc>>) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        INSERT INTO squadov.lol_matches (
            match_uuid,
            platform,
            match_id,
            game_start_time
        )
        VALUES (
            $1,
            $2,
            $3,
            $4
        )
        ",
        match_uuid,
        platform,
        game_id,
        game_start_time,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn create_or_get_match_uuid_for_lol_match(ex: &mut Transaction<'_, Postgres>, platform: &str, game_id: i64, game_start_time: Option<DateTime<Utc>>) -> Result<Uuid, SquadOvError> {
    Ok(match super::get_lol_match_uuid_if_exists(&mut *ex, platform, game_id).await? {
        Some(x) => {
            update_lol_match_game_start_time(&mut *ex, &x, game_start_time).await?;
            x
        },
        None => {
            let match_uuid = matches::create_new_match(&mut *ex, SquadOvGames::LeagueOfLegends).await?;
            link_match_uuid_to_lol_match(&mut *ex, &match_uuid, platform, game_id, game_start_time).await?;
            match_uuid
        }
    })
}

async fn store_lol_match_participant_identities(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, iden: &[LolParticipantDto]) -> Result<(), SquadOvError> {
    if iden.is_empty() {
        return Ok(());
    }

    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_participant_identities (
                match_uuid,
                participant_id,
                account_id,
                current_account_id,
                current_platform_id,
                summoner_name,
                summoner_id,
                platform_id,
                puuid
            )
            VALUES
        ".to_string()
    ];

    for id in iden {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {participant_id},
                    {account_id},
                    {current_account_id},
                    {current_platform_id},
                    {summoner_name},
                    {summoner_id},
                    {platform_id},
                    {puuid}
                )
                ",
                match_uuid=match_uuid,
                participant_id=id.participant_id,
                account_id="NULL",
                current_account_id="NULL",
                current_platform_id="NULL",
                summoner_name=crate::sql_format_string(&id.summoner_name),
                summoner_id=crate::sql_format_string(&id.summoner_id),
                platform_id="NULL",
                puuid=crate::sql_format_string(&id.puuid),
            )
        );
        sql.push(",".to_string());
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;
    Ok(())
}

struct WrappedTeamBan<'a> {
    team_id: i32,
    base: &'a LolBanDto
}

async fn store_lol_match_teams(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, teams: &[LolTeamDto]) -> Result<(), SquadOvError> {
    if teams.is_empty() {
        return Ok(());
    }

    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_teams (
                match_uuid,
                team_id,
                tower_kills,
                rift_herald_kills,
                first_blood,
                inhibitor_kills,
                first_baron,
                first_dragon,
                dragon_kills,
                baron_kills,
                first_inhibitor,
                first_tower,
                first_rift_herald,
                win
            )
            VALUES
        ".to_string()
    ];
    
    let mut all_bans: Vec<WrappedTeamBan> = Vec::new();
    for t in teams {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {team_id},
                    {tower_kills},
                    {rift_herald_kills},
                    {first_blood},
                    {inhibitor_kills},
                    {first_baron},
                    {first_dragon},
                    {dragon_kills},
                    {baron_kills},
                    {first_inhibitor},
                    {first_tower},
                    {first_rift_herald},
                    '{win}'
                )
            ",
                match_uuid=match_uuid,
                team_id=t.team_id,
                tower_kills=t.objectives.tower.kills,
                rift_herald_kills=t.objectives.rift_herald.kills,
                first_blood=crate::sql_format_bool(t.objectives.champion.first),
                inhibitor_kills=t.objectives.inhibitor.kills,
                first_baron=crate::sql_format_bool(t.objectives.baron.first),
                first_dragon=crate::sql_format_bool(t.objectives.dragon.first),
                dragon_kills=t.objectives.dragon.kills,
                baron_kills=t.objectives.baron.kills,
                first_inhibitor=crate::sql_format_bool(t.objectives.inhibitor.first),
                first_tower=crate::sql_format_bool(t.objectives.tower.first),
                first_rift_herald=crate::sql_format_bool(t.objectives.rift_herald.first),
                win=if t.win { "Win" } else { "Fail" },
            )
        );
        sql.push(",".to_string());

        all_bans.extend(t.bans.iter().map(|x| {
            WrappedTeamBan{
                team_id: t.team_id,
                base: x,
            }
        }));
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;

    store_lol_match_team_bans(&mut *ex, match_uuid, &all_bans).await?;
    Ok(())
}

async fn store_lol_match_team_bans<'a>(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, bans: &[WrappedTeamBan<'a>]) -> Result<(), SquadOvError> {
    if bans.is_empty() {
        return Ok(());
    }

    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_bans (
                match_uuid,
                team_id,
                champion_id,
                pick_turn
            )
            VALUES
        ".to_string()
    ];

    for b in bans {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {team_id},
                    {champion_id},
                    {pick_turn}
                )
            ",
                match_uuid=match_uuid,
                team_id=b.team_id,
                champion_id=b.base.champion_id,
                pick_turn=b.base.pick_turn,
            )
        );
        sql.push(",".to_string());
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;
    Ok(())
}

async fn store_lol_match_participants(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, participants: &[LolParticipantDto]) -> Result<(), SquadOvError> {
    if participants.is_empty() {
        return Ok(());
    }

    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_participants (
                match_uuid,
                participant_id,
                champion_id,
                team_id,
                spell1_id,
                spell2_id ,
                champ_level,
                win,
                kills,
                deaths,
                assists,
                item0,
                item1,
                item2,
                item3,
                item4,
                item5,
                item6,
                double_kills,
                triple_kills,
                quadra_kills,
                penta_kills,
                first_blood_kill,
                gold_earned,
                gold_spent,
                neutral_minions_killed_team_jungle,
                neutral_minions_killed_enemy_jungle,
                wards_killed,
                wards_placed,
                vision_wards_bought_in_game,
                sight_wards_bought_in_game,
                neutral_minions_kills,
                total_minions_killed,
                damage_dealt_to_objectives,
                inhibitor_kills,
                turret_kills,
                damage_dealt_to_turrets,
                total_player_score,
                total_score_rank,
                objective_player_score,
                combat_player_score,
                vision_score,
                total_damage_dealt_to_champions,
                physical_damage_dealt_to_champions,
                magic_damage_dealt_to_champions,
                true_damage_dealt_to_champions,
                total_damage_dealt,
                physical_damage_dealt,
                magic_damage_dealt, 
                true_damage_dealt,
                total_damage_taken, 
                physical_damage_taken,
                magical_damage_taken,
                true_damage_taken,
                total_heal,
                damage_self_mitigated,
                lane
            )
            VALUES
        ".to_string()
    ];

    for p in participants {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {participant_id},
                    {champion_id},
                    {team_id},
                    {spell1_id},
                    {spell2_id },
                    {champ_level},
                    {win},
                    {kills},
                    {deaths},
                    {assists},
                    {item0},
                    {item1},
                    {item2},
                    {item3},
                    {item4},
                    {item5},
                    {item6},
                    {double_kills},
                    {triple_kills},
                    {quadra_kills},
                    {penta_kills},
                    {first_blood_kill},
                    {gold_earned},
                    {gold_spent},
                    {neutral_minions_killed_team_jungle},
                    {neutral_minions_killed_enemy_jungle},
                    {wards_killed},
                    {wards_placed},
                    {vision_wards_bought_in_game},
                    {sight_wards_bought_in_game},
                    {neutral_minions_kills},
                    {total_minions_killed},
                    {damage_dealt_to_objectives},
                    {inhibitor_kills},
                    {turret_kills},
                    {damage_dealt_to_turrets},
                    {total_player_score},
                    {total_score_rank},
                    {objective_player_score},
                    {combat_player_score},
                    {vision_score},
                    {total_damage_dealt_to_champions},
                    {physical_damage_dealt_to_champions},
                    {magic_damage_dealt_to_champions},
                    {true_damage_dealt_to_champions},
                    {total_damage_dealt},
                    {physical_damage_dealt},
                    {magic_damage_dealt}, 
                    {true_damage_dealt},
                    {total_damage_taken}, 
                    {physical_damage_taken},
                    {magical_damage_taken},
                    {true_damage_taken},
                    {total_heal},
                    {damage_self_mitigated},
                    '{lane}'
                )
            ",
                match_uuid=match_uuid,
                participant_id=p.participant_id,
                champion_id=p.champion_id,
                team_id=p.team_id,
                spell1_id=p.summoner1_id,
                spell2_id =p.summoner2_id ,
                champ_level=p.champ_level,
                win=p.win,
                kills=p.kills,
                deaths=p.deaths,
                assists=p.assists,
                item0=p.item0,
                item1=p.item1,
                item2=p.item2,
                item3=p.item3,
                item4=p.item4,
                item5=p.item5,
                item6=p.item6,
                double_kills=p.double_kills,
                triple_kills=p.triple_kills,
                quadra_kills=p.quadra_kills,
                penta_kills=p.penta_kills,
                first_blood_kill=p.first_blood_kill,
                gold_earned=p.gold_earned,
                gold_spent=p.gold_spent,
                neutral_minions_killed_team_jungle=p.neutral_minions_killed,
                neutral_minions_killed_enemy_jungle=p.neutral_minions_killed,
                wards_killed=p.wards_killed,
                wards_placed=p.wards_placed,
                vision_wards_bought_in_game=p.vision_wards_bought_in_game,
                sight_wards_bought_in_game=p.sight_wards_bought_in_game,
                neutral_minions_kills=p.neutral_minions_killed,
                total_minions_killed=p.total_minions_killed,
                damage_dealt_to_objectives=p.damage_dealt_to_objectives,
                inhibitor_kills=p.inhibitor_kills,
                turret_kills=p.turret_kills,
                damage_dealt_to_turrets=p.damage_dealt_to_turrets,
                total_player_score=0,
                total_score_rank=0,
                objective_player_score=0,
                combat_player_score=0,
                vision_score=p.vision_score,
                total_damage_dealt_to_champions=p.total_damage_dealt_to_champions,
                physical_damage_dealt_to_champions=p.physical_damage_dealt_to_champions,
                magic_damage_dealt_to_champions=p.magic_damage_dealt_to_champions,
                true_damage_dealt_to_champions=p.true_damage_dealt_to_champions,
                total_damage_dealt=p.total_damage_dealt,
                physical_damage_dealt=p.physical_damage_dealt,
                magic_damage_dealt=p.magic_damage_dealt,
                true_damage_dealt=p.true_damage_dealt,
                total_damage_taken=p.total_damage_taken,
                physical_damage_taken=p.physical_damage_taken,
                magical_damage_taken=p.magic_damage_taken,
                true_damage_taken=p.true_damage_taken,
                total_heal=p.total_heal,
                damage_self_mitigated=p.damage_self_mitigated,
                lane=&p.lane,
            )
        );
        sql.push(",".to_string());
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;
    Ok(())
}

pub async fn store_lol_match_info(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, lol_match: &LolMatchDto) -> Result<(), SquadOvError> {
    // This must ABSOLUTELY fail when it detects a conflict as we should be able safely assume that the data we'll get from the 
    // match history endpoint is the same every time. Therefore, any duplicates here would be redundant and furthermore, the events
    // we store aren't really unique so if we continue we'd get actual duplicated data there.
    let split: Vec<&str> = lol_match.metadata.match_id.split("_").collect();
    sqlx::query!(
        "
        INSERT INTO squadov.lol_match_info (
            match_uuid,
            game_id,
            platform_id,
            queue_id,
            game_type,
            game_duration,
            game_creation,
            season_id,
            game_version,
            map_id,
            game_mode
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8,
            $9,
            $10,
            $11
        )
        ",
        match_uuid,
        &split[1].parse::<i64>()?,
        split[0],
        lol_match.info.queue_id,
        &lol_match.info.game_type,
        // Old league API used to store game duration in seconds, new API gives in milliseconds SOMETIMES.
        // If 'gameEndTimestamp' exists in the data - then this value is seconds.
        // If 'gameEndTimestamp' does not exist in the data - then this value is in milliseconds.
        // ^ This is documented in Riot's match-v5 API documentation.
        if lol_match.info.game_end_timestamp.is_some() {
            lol_match.info.game_duration
        } else {
            lol_match.info.game_duration / 1000
        },
        &lol_match.info.game_creation.ok_or(SquadOvError::BadRequest)?,
        0,
        &lol_match.info.game_version,
        lol_match.info.map_id,
        &lol_match.info.game_mode,
    )
        .execute(&mut *ex)
        .await?;
    
    // The ordering here is pretty essential as players has foreign keys pointing to both teams and identities.
    store_lol_match_participant_identities(&mut *ex, match_uuid, &lol_match.info.participants).await?;
    store_lol_match_teams(&mut *ex, match_uuid, &lol_match.info.teams).await?;
    store_lol_match_participants(&mut *ex, match_uuid, &lol_match.info.participants).await?;
    Ok(())
}

struct WrappedParticipantFrame<'a> {
    timestamp: i64,
    base: &'a LolMatchParticipantFrameDto
}

struct WrappedEvent<'a> {
    base: &'a LolMatchEventDto
}

async fn store_lol_match_timeline_participant_frames<'a>(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, frames: &[WrappedParticipantFrame<'a>]) -> Result<(), SquadOvError> {
    if frames.is_empty() {
        return Ok(());
    }
    
    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_timeline_participant_frames (
                match_uuid,
                timestamp,
                participant_id,
                minions_killed,
                total_gold,
                level,
                xp,
                current_gold,
                jungle_minions_killed,
                x,
                y
            )
            VALUES
        ".to_string()
    ];

    for f in frames {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {timestamp},
                    {participant_id},
                    {minions_killed},
                    {total_gold},
                    {level},
                    {xp},
                    {current_gold},
                    {jungle_minions_killed},
                    {x},
                    {y}
                )
            ",
                match_uuid=match_uuid,
                timestamp=f.timestamp,
                participant_id=f.base.participant_id,
                minions_killed=f.base.minions_killed,
                total_gold=f.base.total_gold,
                level=f.base.level,
                xp=f.base.xp,
                current_gold=f.base.current_gold,
                jungle_minions_killed=f.base.jungle_minions_killed,
                x=crate::sql_format_option_value(&f.base.position.as_ref().and_then(|x| { Some(x.x) })),
                y=crate::sql_format_option_value(&f.base.position.as_ref().and_then(|x| { Some(x.x) })),
            )
        );
        sql.push(",".to_string());
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;
    Ok(())
}

async fn store_lol_match_timeline_events<'a>(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, events: &[WrappedEvent<'a>]) -> Result<(), SquadOvError> {
    if events.is_empty() {
        return Ok(());
    }

    let mut sql = vec![
        "
            INSERT INTO squadov.lol_match_timeline_events (
                match_uuid,
                timestamp,
                real_type,
                lane_type,
                skill_slot,
                ascended_type,
                creator_id,
                after_id,
                event_type,
                level_up_type,
                ward_type,
                participant_id,
                tower_type,
                item_id,
                before_id,
                monster_type,
                monster_sub_type,
                team_id,
                x,
                y,
                killer_id,
                assisting_participant_ids,
                building_type,
                victim_id
            )
            VALUES
        ".to_string()
    ];

    for e in events {
        sql.push(
            format!("
                (
                    '{match_uuid}',
                    {timestamp},
                    '{real_type}',
                    {lane_type},
                    {skill_slot},
                    {ascended_type},
                    {creator_id},
                    {after_id},
                    {event_type},
                    {level_up_type},
                    {ward_type},
                    {participant_id},
                    {tower_type},
                    {item_id},
                    {before_id},
                    {monster_type},
                    {monster_sub_type},
                    {team_id},
                    {x},
                    {y},
                    {killer_id},
                    {assisting_participant_ids},
                    {building_type},
                    {victim_id}
                )
            ",
                match_uuid=match_uuid,
                timestamp=e.base.timestamp,
                real_type=&e.base.real_type,
                lane_type=crate::sql_format_option_string(&e.base.lane_type),
                skill_slot=crate::sql_format_option_value(&e.base.skill_slot),
                ascended_type=crate::sql_format_option_string(&e.base.ascended_type),
                creator_id=crate::sql_format_option_value(&e.base.creator_id),
                after_id=crate::sql_format_option_value(&e.base.after_id),
                event_type=crate::sql_format_option_string(&e.base.event_type),
                level_up_type=crate::sql_format_option_string(&e.base.level_up_type),
                ward_type=crate::sql_format_option_string(&e.base.ward_type),
                participant_id=crate::sql_format_option_value(&e.base.participant_id),
                tower_type=crate::sql_format_option_string(&e.base.tower_type),
                item_id=crate::sql_format_option_value(&e.base.item_id),
                before_id=crate::sql_format_option_value(&e.base.before_id),
                monster_type=crate::sql_format_option_string(&e.base.monster_type),
                monster_sub_type=crate::sql_format_option_string(&e.base.monster_sub_type),
                team_id=crate::sql_format_option_value(&e.base.team_id),
                x=crate::sql_format_option_value(&e.base.position.as_ref().and_then(|x| { Some(x.x) })),
                y=crate::sql_format_option_value(&e.base.position.as_ref().and_then(|x| { Some(x.y) })),
                killer_id=crate::sql_format_option_value(&e.base.killer_id),
                assisting_participant_ids=crate::sql_format_integer_array(&e.base.assisting_participant_ids.as_ref().unwrap_or(&vec![])),
                building_type=crate::sql_format_option_string(&e.base.building_type),
                victim_id=crate::sql_format_option_value(&e.base.victim_id),
            )
        );
        sql.push(",".to_string());
    }

    sql.truncate(sql.len() - 1);
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;
    Ok(())
}

pub async fn store_lol_match_timeline_info(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, timeline: &LolMatchTimelineInfoDto) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        INSERT INTO squadov.lol_match_timeline (
            match_uuid,
            frame_interval
        )
        VALUES (
            $1,
            $2
        )
        ",
        match_uuid,
        timeline.frame_interval,
    )
        .execute(&mut *ex)
        .await?;

    let mut frames: Vec<WrappedParticipantFrame> = Vec::new();
    let mut events: Vec<WrappedEvent> = Vec::new();

    for f in &timeline.frames {
        frames.extend(f.participant_frames.iter().map(|(_, x)| {
            WrappedParticipantFrame {
                timestamp: f.timestamp,
                base: x,
            }
        }));

        events.extend(f.events.iter().map(|x| {
            WrappedEvent {
                base: x,
            }
        }));
    }

    store_lol_match_timeline_participant_frames(&mut *ex, match_uuid, &frames).await?;
    store_lol_match_timeline_events(&mut *ex, match_uuid, &events).await?;
    Ok(())
}