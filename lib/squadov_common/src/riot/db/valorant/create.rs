use crate::{
    SquadOvError,
    riot::games::valorant::{
        ValorantMatchDto,
        ValorantMatchInfoDto,
        ValorantMatchPlayerDto,
        ValorantMatchTeamDto,
        ValorantMatchRoundResultDto,
        FlatValorantMatchKillDto,
        FlatValorantMatchDamageDto,
        FlatValorantMatchEconomyDto,
        FlatValorantMatchPlayerRoundStatsDto,
    },
    matches
};
use sqlx::{Transaction, Postgres};
use uuid::Uuid;
use std::cmp::Ordering;

async fn link_match_uuid_to_valorant_match_id(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, match_id: &str) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        INSERT INTO squadov.valorant_match_uuid_link (
            match_uuid,
            match_id
        )
        VALUES (
            $1,
            $2
        )
        ",
        match_uuid,
        match_id,
    )
        .execute(ex)
        .await?;
    Ok(())
}

async fn store_valorant_match_info_dto(ex: &mut Transaction<'_, Postgres>, info: &ValorantMatchInfoDto) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        INSERT INTO squadov.valorant_matches (
            match_id,
            map_id,
            game_length_millis,
            server_start_time_utc,
            provisioning_flow_id,
            game_mode,
            is_ranked,
            season_id
        ) VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8
        )
        ",
        info.match_id,
        info.map_id,
        info.game_length_millis,
        info.server_start_time_utc,
        info.provisioning_flow_id,
        info.game_mode,
        info.is_ranked,
        info.season_id,
    )
        .execute(ex)
        .await?;
    Ok(())
}

async fn store_valorant_match_player_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, info: &[ValorantMatchPlayerDto]) -> Result<(), SquadOvError> {
    if info.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_players (
            match_id,
            team_id,
            puuid,
            character_id,
            competitive_tier,
            total_combat_score,
            rounds_played,
            kills,
            deaths,
            assists
        )
        VALUES
    "));

    for m in info {
        sql.push(format!("(
            '{match_id}',
            '{team_id}',
            '{puuid}',
            '{character_id}',
            {competitive_tier},
            {total_combat_score},
            {rounds_played},
            {kills},
            {deaths},
            {assists}
        )",
            match_id=match_id,
            team_id=&m.team_id,
            puuid=&m.puuid,
            character_id=&m.character_id,
            competitive_tier=m.competitive_tier,
            total_combat_score=m.stats.score,
            rounds_played=m.stats.rounds_played,
            kills=m.stats.kills,
            deaths=m.stats.deaths,
            assists=m.stats.assists
        ));
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_team_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, info: &[ValorantMatchTeamDto]) -> Result<(), SquadOvError> {
    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_teams (
            match_id,
            team_id,
            won,
            rounds_won,
            rounds_played,
            num_points
        )
        VALUES
    "));

    for m in info {
        sql.push(format!("(
            '{match_id}',
            '{team_id}',
            {won},
            {rounds_won},
            {rounds_played},
            {num_points}
        )",
            match_id=match_id,
            team_id=&m.team_id,
            won=crate::sql_format_bool(m.won),
            rounds_won=m.rounds_won,
            rounds_played=m.rounds_played,
            num_points=m.num_points
        ));
        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_round_result_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, info: &[ValorantMatchRoundResultDto]) -> Result<(), SquadOvError> {
    if info.is_empty() {
        return Ok(());
    }

    let mut round_stats: Vec<FlatValorantMatchPlayerRoundStatsDto> = Vec::new();
    let mut kills: Vec<FlatValorantMatchKillDto> = Vec::new();
    let mut damage: Vec<FlatValorantMatchDamageDto> = Vec::new();
    let mut econ: Vec<FlatValorantMatchEconomyDto> = Vec::new();

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_rounds (
            match_id,
            round_num,
            plant_round_time,
            planter_puuid,
            defuse_round_time,
            defuser_puuid,
            team_round_winner
        )
        VALUES
    "));

    for m in info {
        sql.push(format!("(
            '{match_id}',
            {round_num},
            {plant_round_time},
            {planter_puuid},
            {defuse_round_time},
            {defuser_puuid},
            '{team_round_winner}'
        )",
            match_id=match_id,
            round_num=m.round_num,
            plant_round_time=crate::sql_format_option_value(&m.plant_round_time),
            planter_puuid=crate::sql_format_option_string(&m.bomb_planter),
            defuse_round_time=crate::sql_format_option_value(&m.defuse_round_time),
            defuser_puuid=crate::sql_format_option_string(&m.bomb_defuser),
            team_round_winner=&m.winning_team
        ));
        sql.push(String::from(","));

        let (s, k, d, e) = m.flatten(m.round_num);
        round_stats.extend(s.into_iter());
        kills.extend(k.into_iter());
        damage.extend(d.into_iter());
        econ.extend(e.into_iter());
    }
    
    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(&mut *ex).await?;

    store_valorant_match_flat_valorant_match_player_round_stats_dto(ex, match_id, &round_stats).await?;
    store_valorant_match_flat_valorant_match_kill_dto(ex, match_id, &kills).await?;
    store_valorant_match_flat_valorant_match_damage_dto(ex, match_id, &damage).await?;
    store_valorant_match_flat_valorant_match_economy_dto(ex, match_id, &econ).await?;

    Ok(())
}

async fn store_valorant_match_flat_valorant_match_player_round_stats_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, stats: &[FlatValorantMatchPlayerRoundStatsDto]) -> Result<(), SquadOvError> {
    if stats.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_round_player_stats (
            match_id,
            round_num,
            puuid,
            combat_score
        )
        VALUES
    "));

    for st in stats {
        sql.push(format!("(
            '{match_id}',
            {round_num},
            '{puuid}',
            {combat_score}
        )",
            match_id=match_id,
            round_num=st.round_num,
            puuid=&st.puuid,
            combat_score=st.score
        ));

        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_flat_valorant_match_kill_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, kills: &[FlatValorantMatchKillDto]) -> Result<(), SquadOvError> {
    if kills.is_empty() {
        return Ok(());
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_kill (
            match_id,
            round_num,
            killer_puuid,
            victim_puuid,
            time_since_game_start_millis,
            time_since_round_start_millis,
            damage_type,
            damage_item,
            is_secondary_fire,
            assistants
        )
        VALUES
    "));

    for m in kills {
        sql.push(format!("(
            '{match_id}',
            {round_num},
            {killer_puuid},
            '{victim_puuid}',
            {time_since_game_start_millis},
            {time_since_round_start_millis},
            '{damage_type}',
            '{damage_item}',
            {is_secondary_fire},
            {assistants}
        )",
            match_id=match_id,
            round_num=m.round_num,
            killer_puuid=crate::sql_format_option_string(&m.base.killer),
            victim_puuid=&m.base.victim,
            time_since_game_start_millis=m.base.time_since_game_start_millis,
            time_since_round_start_millis=m.base.time_since_round_start_millis,
            damage_type=m.base.finishing_damage.damage_type,
            damage_item=m.base.finishing_damage.damage_item,
            is_secondary_fire=m.base.finishing_damage.is_secondary_fire_mode,
            assistants=crate::sql_format_varchar_array(&m.base.assistants),
        ));

        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_flat_valorant_match_damage_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, all_damage: &[FlatValorantMatchDamageDto]) -> Result<(), SquadOvError> {
    if all_damage.is_empty() {
        return Ok(());
    }

    let mut sql : Vec<String> = Vec::new();

    // Duplicate comment from the V0012.1__ValorantDuplicateDamage.sql migration:actix_web
    // This sequence ID is LOW KEY INSANE. Effectively we're assuming that we're going to be inserting
    // player damage into the table in the same order EVERY TIME so that the 5th damage insertion is going
    // to be the same assuming we parse the same match history JSON multiple times. Why do we need to do that?
    // Because Valorant's damage information is NOT UNIQUE. It's possible for the game to give us multiple
    // damage dealt objects from one player to another in a single round. Thus we need to find some way of being
    // able to detect if we're trying to insert the same damage element. Hence this sequence_id. It'll be up
    // to the application to create a temporary sequence AND USE IT in the insertion. Y I K E S.
    let random_sequence_name = format!("dmgseq{}", Uuid::new_v4().to_simple().to_string());
    sqlx::query(&format!("CREATE TEMPORARY SEQUENCE {}", &random_sequence_name)).execute(&mut *ex).await?;

    sql.push(String::from("
        INSERT INTO squadov.valorant_match_damage (
            match_id,
            round_num,
            instigator_puuid,
            receiver_puuid,
            damage,
            legshots,
            bodyshots,
            headshots,
            sequence_id
        )
        VALUES
    "));

    // Player damage vector needs to be sorted properly to match the migration from before we used
    // a sequence to identify unique damage. Sort order: round num, 
    // instigator_puuid, receiver_puuid, damage, legshots, bodyshots, headshots.
    // All in ascending order.
    let mut sorted_data: Vec<FlatValorantMatchDamageDto> = all_damage.iter().cloned().collect();
    sorted_data.sort_by(|a, b| {
        if a.round_num < b.round_num {
            return Ordering::Less;
        } else if a.round_num > b.round_num {
            return Ordering::Greater;
        }

        if a.instigator < b.instigator {
            return Ordering::Less;
        } else if a.instigator > b.instigator {
            return Ordering::Greater;
        }

        if a.base.receiver < b.base.receiver {
            return Ordering::Less;
        } else if a.base.receiver > b.base.receiver {
            return Ordering::Greater;
        }

        if a.base.damage < b.base.damage {
            return Ordering::Less;
        } else if a.base.damage > b.base.damage {
            return Ordering::Greater;
        }

        if a.base.legshots < b.base.legshots {
            return Ordering::Less;
        } else if a.base.legshots > b.base.legshots {
            return Ordering::Greater;
        }

        if a.base.bodyshots < b.base.bodyshots {
            return Ordering::Less;
        } else if a.base.bodyshots > b.base.bodyshots {
            return Ordering::Greater;
        }

        if a.base.headshots < b.base.headshots {
            return Ordering::Less;
        } else if a.base.headshots > b.base.headshots {
            return Ordering::Greater;
        }

        return Ordering::Equal;
    });

    for dmg in sorted_data {
        sql.push(format!("(
            '{match_id}',
            {round_num},
            '{instigator_puuid}',
            '{receiver_puuid}',
            {damage},
            {legshots},
            {bodyshots},
            {headshots},
            NEXTVAL('{seq}')
        )",
            match_id=match_id,
            round_num=dmg.round_num,
            instigator_puuid=&dmg.instigator,
            receiver_puuid=&dmg.base.receiver,
            damage=dmg.base.damage,
            legshots=dmg.base.legshots,
            bodyshots=dmg.base.bodyshots,
            headshots=dmg.base.headshots,
            seq=&random_sequence_name,
        ));

        sql.push(String::from(","));
    }

    // This is responsible for removing the trailing comma.
    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_flat_valorant_match_economy_dto(ex: &mut Transaction<'_, Postgres>, match_id: &str, all_econ: &[FlatValorantMatchEconomyDto]) -> Result<(), SquadOvError> {
    if all_econ.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_round_player_loadout (
            match_id,
            round_num,
            puuid,
            loadout_value,
            remaining_money,
            spent_money,
            weapon,
            armor
        )
        VALUES
    "));

    for econ in all_econ {
        sql.push(format!("(
            '{match_id}',
            {round_num},
            '{puuid}',
            {loadout_value},
            {remaining_money},
            {spent_money},
            '{weapon}',
            '{armor}'
        )",
            match_id=match_id,
            round_num=econ.round_num,
            puuid=&econ.puuid,
            loadout_value=econ.base.loadout_value,
            remaining_money=econ.base.remaining,
            spent_money=econ.base.spent,
            weapon=econ.base.weapon,
            armor=econ.base.armor
        ));

        sql.push(String::from(","));
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

pub async fn store_valorant_match_dto(ex: &mut Transaction<'_, Postgres>, valorant_match: &ValorantMatchDto) -> Result<(), SquadOvError> {
    store_valorant_match_info_dto(ex, &valorant_match.match_info).await?;
    // The order here must be 1) Teams 2) Players and 3) Round Results.
    // Players have a reference to what team they're on and round results have references to which player is relevant it's for.
    // These references are enforced in the database.
    store_valorant_match_team_dto(ex, &valorant_match.match_info.match_id, &valorant_match.teams).await?;
    store_valorant_match_player_dto(ex, &valorant_match.match_info.match_id, &valorant_match.players).await?;
    store_valorant_match_round_result_dto(ex, &valorant_match.match_info.match_id, &valorant_match.round_results).await?;
    Ok(())
}

pub async fn create_or_get_match_uuid_for_valorant_match(ex: &mut Transaction<'_, Postgres>, match_id: &str) -> Result<Uuid, SquadOvError> {
    Ok(match super::get_valorant_match_uuid_if_exists(&mut *ex, match_id).await? {
        Some(x) => x,
        None => {
            let match_uuid = matches::create_new_match(&mut *ex).await?;
            link_match_uuid_to_valorant_match_id(&mut *ex, &match_uuid, match_id).await?;
            match_uuid
        }
    })
}