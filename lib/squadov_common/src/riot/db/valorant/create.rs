use crate::{
    SquadOvError,
    games::SquadOvGames,
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
        ValorantMatchFilterEvents,
    },
    matches
};
use sqlx::{Transaction, Postgres, Executor};
use uuid::Uuid;
use std::cmp::Ordering;
use std::collections::HashSet;

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

async fn store_valorant_match_info_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, info: &ValorantMatchInfoDto) -> Result<(), SquadOvError> {
    sqlx::query!(
        "
        INSERT INTO squadov.valorant_matches (
            match_uuid,
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
        match_uuid,
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

async fn store_valorant_match_player_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, info: &[ValorantMatchPlayerDto]) -> Result<(), SquadOvError> {
    if info.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_players (
            match_uuid,
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

    let mut added = 0;
    for m in info {
        if let Some(character_id) = &m.character_id {
            if let Some(stats) = &m.stats {
                sql.push(format!("(
                    '{match_uuid}',
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
                    match_uuid=match_uuid,
                    team_id=&m.team_id,
                    puuid=&m.puuid,
                    character_id=&character_id,
                    competitive_tier=m.competitive_tier,
                    total_combat_score=stats.score,
                    rounds_played=stats.rounds_played,
                    kills=stats.kills,
                    deaths=stats.deaths,
                    assists=stats.assists
                ));
                sql.push(String::from(","));
                added += 1;
            }
        }
    }

    if added == 0 {
        return Ok(());
    }

    sql.truncate(sql.len() - 1);
    sql.push(String::from(" ON CONFLICT DO NOTHING"));
    sqlx::query(&sql.join("")).execute(ex).await?;
    Ok(())
}

async fn store_valorant_match_team_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, info: &[ValorantMatchTeamDto]) -> Result<(), SquadOvError> {
    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_teams (
            match_uuid,
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
            '{match_uuid}',
            '{team_id}',
            {won},
            {rounds_won},
            {rounds_played},
            {num_points}
        )",
            match_uuid=match_uuid,
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

async fn store_valorant_match_round_result_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, info: &[ValorantMatchRoundResultDto]) -> Result<(), SquadOvError> {
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
            match_uuid,
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
            '{match_uuid}',
            {round_num},
            {plant_round_time},
            {planter_puuid},
            {defuse_round_time},
            {defuser_puuid},
            '{team_round_winner}'
        )",
            match_uuid=match_uuid,
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

    store_valorant_match_flat_valorant_match_player_round_stats_dto(ex, match_uuid, &round_stats).await?;
    store_valorant_match_flat_valorant_match_kill_dto(ex, match_uuid, &kills).await?;
    store_valorant_match_flat_valorant_match_damage_dto(ex, match_uuid, &damage).await?;
    store_valorant_match_flat_valorant_match_economy_dto(ex, match_uuid, &econ).await?;

    Ok(())
}

async fn store_valorant_match_flat_valorant_match_player_round_stats_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, stats: &[FlatValorantMatchPlayerRoundStatsDto]) -> Result<(), SquadOvError> {
    if stats.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_round_player_stats (
            match_uuid,
            round_num,
            puuid,
            combat_score
        )
        VALUES
    "));

    for st in stats {
        sql.push(format!("(
            '{match_uuid}',
            {round_num},
            '{puuid}',
            {combat_score}
        )",
            match_uuid=match_uuid,
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

async fn store_valorant_match_flat_valorant_match_kill_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, kills: &[FlatValorantMatchKillDto]) -> Result<(), SquadOvError> {
    if kills.is_empty() {
        return Ok(());
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_kill (
            match_uuid,
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
            '{match_uuid}',
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
            match_uuid=match_uuid,
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

async fn store_valorant_match_flat_valorant_match_damage_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, all_damage: &[FlatValorantMatchDamageDto]) -> Result<(), SquadOvError> {
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
            match_uuid,
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
            '{match_uuid}',
            {round_num},
            '{instigator_puuid}',
            '{receiver_puuid}',
            {damage},
            {legshots},
            {bodyshots},
            {headshots},
            NEXTVAL('{seq}')
        )",
            match_uuid=match_uuid,
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

async fn store_valorant_match_flat_valorant_match_economy_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, all_econ: &[FlatValorantMatchEconomyDto]) -> Result<(), SquadOvError> {
    if all_econ.is_empty() {
        return Ok(())
    }

    let mut sql : Vec<String> = Vec::new();
    sql.push(String::from("
        INSERT INTO squadov.valorant_match_round_player_loadout (
            match_uuid,
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
            '{match_uuid}',
            {round_num},
            '{puuid}',
            {loadout_value},
            {remaining_money},
            {spent_money},
            '{weapon}',
            '{armor}'
        )",
            match_uuid=match_uuid,
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

pub async fn store_valorant_match_dto(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, valorant_match: &ValorantMatchDto) -> Result<(), SquadOvError> {
    store_valorant_match_info_dto(ex, match_uuid, &valorant_match.match_info).await?;
    // The order here must be 1) Teams 2) Players and 3) Round Results.
    // Players have a reference to what team they're on and round results have references to which player is relevant it's for.
    // These references are enforced in the database.
    store_valorant_match_team_dto(ex, match_uuid,&valorant_match.teams).await?;
    store_valorant_match_player_dto(ex, match_uuid, &valorant_match.players).await?;
    store_valorant_match_round_result_dto(ex, match_uuid, &valorant_match.round_results).await?;
    Ok(())
}

async fn get_agent_list_for_players_on_team<'a, T>(ex: T, match_uuid: &Uuid, team: Option<&String>) -> Result<Vec<String>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT character_id
            FROM squadov.valorant_match_players
            WHERE match_uuid = $1
                AND ($2::VARCHAR IS NULL OR team_id = $2)
            ",
            match_uuid,
            team,
        )
            .fetch_all(ex)
            .await?
            .into_iter()
            .map(|x| {
                x.character_id
            })
            .collect::<Vec<String>>()
    )
}

pub async fn cache_valorant_match_information(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid) -> Result<(), SquadOvError> {
    // We need to get which agents are on what team and create a regex-searchable string representation.
    // Note that in the case where we're playing deathmatch where there's no teams (and everyone's team id is equivalent to their puuid),
    // then we will dump everyone into "team 0".
    let teams = sqlx::query!(
        "
        SELECT DISTINCT team_id
        FROM squadov.valorant_match_players
        WHERE match_uuid = $1
        ",
        match_uuid
    )
        .fetch_all(&mut *ex)
        .await?
        .into_iter()
        .map(|x| {
            x.team_id
        })
        .collect::<Vec<String>>();

    let t0_agents: String;
    let t1_agents: String;
    if teams.len() == 2 {
        t0_agents =  format!(",{},", get_agent_list_for_players_on_team(&mut *ex, match_uuid, teams.get(0)).await?.join(","));
        t1_agents = format!(",{},", get_agent_list_for_players_on_team(&mut *ex, match_uuid, teams.get(1)).await?.join(","));
    } else {
        t0_agents = format!(",{},", get_agent_list_for_players_on_team(&mut *ex, match_uuid, None).await?.join(","));
        t1_agents = String::new();
    }

    sqlx::query!(
        "
        INSERT INTO squadov.valorant_match_computed_data (
            match_uuid,
            t0_agents,
            t1_agents
        ) VALUES (
            $1,
            $2,
            $3
        ) ON CONFLICT (match_uuid) DO UPDATE SET
            t0_agents = EXCLUDED.t0_agents,
            t1_agents = EXCLUDED.t1_agents
        ",
        match_uuid,
        &t0_agents,
        &t1_agents,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn cache_valorant_player_pov_information(ex: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
    let cached_data = sqlx::query!(
        "
        SELECT
            vmp.character_id,
            vmp.competitive_tier,
            vmt.won
        FROM squadov.valorant_match_players AS vmp
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = vmp.puuid
        INNER JOIN squadov.valorant_match_teams AS vmt
            ON vmt.team_id = vmp.team_id
        WHERE vmp.match_uuid = $1
            AND ral.user_id = $2
        ",
        match_uuid,
        user_id,
    )
        .fetch_one(&mut *ex)
        .await?;

    let cached_per_round_data = sqlx::query!(
        r#"
        SELECT
            vmk.round_num,
            COUNT(vmk.victim_puuid) AS "kills!"
        FROM squadov.valorant_match_players AS vmp
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = vmp.puuid
        LEFT JOIN squadov.valorant_match_kill AS vmk
            ON vmk.match_uuid = vmp.match_uuid
                AND vmk.killer_puuid = vmp.puuid
        WHERE vmp.match_uuid = $1
            AND ral.user_id = $2
        GROUP BY vmk.round_num
        "#,
        match_uuid,
        user_id
    )
        .fetch_all(&mut *ex)
        .await?;

    let mut events: HashSet<ValorantMatchFilterEvents> = HashSet::new();
    for round in cached_per_round_data {
        if round.kills >= 5 {
            events.insert(ValorantMatchFilterEvents::PentaKill);
        } else if round.kills >= 4 {
            events.insert(ValorantMatchFilterEvents::QuadraKill);
        } else if round.kills >= 3 {
            events.insert(ValorantMatchFilterEvents::TripleKill);
        } else if round.kills >= 2 {
            events.insert(ValorantMatchFilterEvents::DoubleKill);
        }
    }

    sqlx::query!(
        "
        INSERT INTO squadov.valorant_match_pov_computed_data (
            match_uuid,
            user_id,
            pov_agent,
            rank,
            key_events,
            winner
        ) VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6
        ) ON CONFLICT (match_uuid, user_id) DO UPDATE SET
            pov_agent = EXCLUDED.pov_agent,
            rank = EXCLUDED.rank,
            key_events = EXCLUDED.key_events,
            winner = EXCLUDED.winner
        ",
        match_uuid,
        user_id,
        &cached_data.character_id,
        &cached_data.competitive_tier,
        &events.into_iter().map(|x| { x as i32 }).collect::<Vec<i32>>(),
        &cached_data.won,
    )
        .execute(&mut *ex)
        .await?;
    Ok(())
}

pub async fn create_or_get_match_uuid_for_valorant_match(ex: &mut Transaction<'_, Postgres>, match_id: &str) -> Result<Uuid, SquadOvError> {
    Ok(match super::get_valorant_match_uuid_if_exists(&mut *ex, match_id).await? {
        Some(x) => x,
        None => {
            let match_uuid = matches::create_new_match(&mut *ex, SquadOvGames::Valorant).await?;
            link_match_uuid_to_valorant_match_id(&mut *ex, &match_uuid, match_id).await?;
            match_uuid
        }
    })
}