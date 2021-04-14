use actix_web::{web, FromRequest, HttpResponse};
use actix_web::dev::{HttpServiceFactory};
use super::auth;
use super::v1;
use super::oembed;
use super::meta;
use super::access;
use super::graphql;
use super::admin;
use std::vec::Vec;
use std::boxed::Box;
use squadov_common::{AimlabTask, SquadOvError};

async fn health_check() -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().finish())
}

pub fn create_service(graphql_debug: bool) -> impl HttpServiceFactory {
    let mut scope = web::scope("")
        .route("/oembed", web::get().to(oembed::oembed_handler))
        .route("/meta", web::get().to(meta::meta_handler))
        .route("/healthz", web::get().to(health_check))
        .service(
            web::scope("/admin")
                .wrap(access::ApiAccess::new(
                    Box::new(access::ShareTokenAccessRestricter{}),
                ))
                .wrap(access::ApiAccess::new(
                    Box::new(access::AdminAccessChecker{}),
                ))
                .wrap(auth::ApiSessionValidator{})
                .service(
                    web::scope("/analytics")
                        .route("/daily", web::get().to(admin::get_daily_analytics_handler))
                        .route("/monthly", web::get().to(admin::get_monthly_analytics_handler))
                )
        )
        .service(
            web::scope("/auth")
                .route("/login", web::post().to(auth::login_handler))
                .route("/logout", web::post().to(auth::logout_handler))
                .route("/register", web::post().to(auth::register_handler))
                .route("/forgotpw", web::get().to(auth::forgot_pw_handler))
                .route("/forgotpw/change", web::post().to(auth::forgot_pw_change_handler))
                .route("/verify", web::post().to(auth::verify_email_handler))
                .service(
                    // This needs to not be protected by the session validator as the session may be
                    // expired!
                    web::resource("/session/heartbeat")
                        .route(web::post().to(v1::refresh_user_session_handler))
                )
                .service(
                    web::scope("/oauth")
                        .route("/riot", web::post().to(v1::handle_riot_oauth_callback_handler))
                )
                .service(
                    // These are the only two endpoints where the user needs to provide a valid session to use.
                    web::scope("")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .wrap(auth::ApiSessionValidator{})
                        .route("/verify", web::get().to(auth::check_verify_email_handler))
                        .route("/verify/resend", web::post().to(auth::resend_verify_email_handler))
                )
        )
        .service(
            web::scope("/ws")
                .route("/status/{user_id}", web::get().to(v1::get_user_status_handler))
        )
        .service(
            // TODO: More generic signature verification here?
            web::scope("/public")
                .service(
                    web::scope("/squad")
                        .service(
                            web::scope("/{squad_id}")
                                .service(
                                    web::scope("/invite/{invite_uuid}")
                                        .route("/accept", web::post().to(v1::public_accept_squad_invite_handler))
                                        .route("/reject", web::post().to(v1::public_reject_squad_invite_handler))
                                )
                        )
                )
                .service(
                    web::scope("/share/{access_token_id}")
                        .route("/exchange", web::post().to(v1::exchange_access_token_id_handler))
                )
                .service(
                    web::scope("/landing")
                        .route("/visit", web::get().to(v1::public_landing_visit_handler))
                        .route("/download", web::get().to(v1::public_landing_download_handler))
                )
        )
        .service(
            web::scope("/v1")
                .wrap(access::ApiAccess::new(
                    Box::new(access::ShareTokenAccessRestricter{}),
                ))
                .wrap(auth::ApiSessionValidator{})
                .service(
                    web::scope("/bug")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("", web::post().to(v1::create_bug_report_handler))
                )
                .service(
                    web::scope("/kafka")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("/info", web::get().to(v1::get_kafka_info_handler))
                )
                .service(
                    web::scope("/match/{match_uuid}")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("/share", web::post().to(v1::create_match_share_signature_handler))
                        .route("/favorite", web::post().to(v1::favorite_match_handler))
                        .route("/favorite", web::get().to(v1::check_favorite_match_handler))
                        .route("/favorite", web::delete().to(v1::remove_favorite_match_handler))
                )
                .service(
                    web::scope("/users")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .service(
                            web::scope("/me")
                                .service(
                                    web::resource("/profile")
                                        .route(web::get().to(v1::get_current_user_profile_handler))
                                )
                                .service(
                                    web::resource("/notifications")
                                        .route(web::get().to(v1::get_current_user_notifications_handler))
                                )
                                .route("/active", web::post().to(v1::mark_user_active_endpoint_handler))
                                .route("/playtime", web::get().to(v1::get_user_recorded_playtime_handler))
                                .route("/recent", web::get().to(v1::get_recent_matches_for_me_handler))
                                .route("/referral", web::get().to(v1::get_user_me_referral_link_handler))
                                .route("/squadmates", web::get().to(v1::get_user_squadmates_handler))
                        )
                        .service(
                            web::scope("/{user_id}")
                                .route("/features", web::get().to(v1::get_user_feature_flags_handler))
                                .service(
                                    web::scope("/profile")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::SameSquadAccessChecker{
                                                obtainer: access::UserIdPathSetObtainer{
                                                    key: "user_id"
                                                },
                                            }),
                                        ))
                                        .route("", web::get().to(v1::get_user_profile_handler))
                                )
                                .service(
                                    web::scope("/squads")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::UserSpecificAccessChecker{
                                                obtainer: access::UserIdPathSetObtainer{
                                                    key: "user_id"
                                                },
                                            }),
                                        ))
                                        .route("", web::get().to(v1::get_user_squads_handler))
                                        .route("/invites", web::get().to(v1::get_user_squad_invites_handler))
                                )
                                .service(
                                    web::scope("/oauth")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::UserSpecificAccessChecker{
                                                obtainer: access::UserIdPathSetObtainer{
                                                    key: "user_id"
                                                },
                                            }),
                                        ))
                                        .route("/rso", web::get().to(v1::get_user_rso_auth_url_handler))
                                )
                                .service(
                                    web::scope("/accounts")
                                        .service(
                                            web::scope("/riot")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::UserSpecificAccessChecker{
                                                        obtainer: access::UserIdPathSetObtainer{
                                                            key: "user_id"
                                                        },
                                                    }),
                                                ).verb_override(
                                                    "GET",
                                                    Box::new(access::SameSquadAccessChecker{
                                                        obtainer: access::UserIdPathSetObtainer{
                                                            key: "user_id"
                                                        },
                                                    })
                                                ))
                                                .service(
                                                    web::scope("/valorant")
                                                        .route("/puuid/{puuid}", web::get().to(v1::get_riot_valorant_account_handler))
                                                        .route("/account", web::post().to(v1::verify_valorant_account_ownership_handler))
                                                        .route("", web::get().to(v1::list_riot_valorant_accounts_handler))
                                                )
                                                .service(
                                                    web::scope("/lol")
                                                        .route("/account", web::post().to(v1::verify_lol_summoner_ownership_handler))
                                                        .route("", web::get().to(v1::list_riot_lol_accounts_handler))
                                                )
                                                .service(
                                                    web::scope("/tft")
                                                        .route("/account", web::post().to(v1::verify_tft_summoner_ownership_handler))
                                                        .route("", web::get().to(v1::list_riot_tft_accounts_handler))
                                                )
                                                .service(
                                                    web::scope("/generic/{puuid}")
                                                        .wrap(access::ApiAccess::new(
                                                            Box::new(access::RiotValorantAccountAccessChecker{
                                                                obtainer: access::RiotValorantAccountPathObtainer{
                                                                    user_id_key: "user_id",
                                                                    puuid_key: "puuid",
                                                                },
                                                            }),
                                                        ))
                                                        .route("", web::post().to(v1::refresh_riot_account_from_puuid_handler))
                                                        .route("", web::delete().to(v1::delete_riot_account_handler))
                                                )
                                        )
                                )
                        )
                )
                .service(
                    web::scope("/lol")
                        .route("", web::post().to(v1::create_lol_match_handler))
                        .service(
                            web::scope("/match/{match_uuid}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::LolMatchAccessChecker{
                                        obtainer: access::LolMatchUuidPathObtainer{
                                            match_uuid_key: "match_uuid",
                                        },
                                    }),
                                ))
                                .route("", web::post().to(v1::finish_lol_match_handler))
                                .route("", web::get().to(v1::get_lol_match_handler))
                                .service(
                                    web::scope("/user/{user_id}")
                                        .route("/vods", web::get().to(v1::get_lol_match_user_accessible_vod_handler))
                                )
                        )
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::DenyShareTokenAccess{}),
                                ))
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
                                ))
                                .route("/backfill", web::post().to(v1::request_lol_match_backfill_handler))
                                .service(
                                    web::scope("/accounts/{puuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::RiotValorantAccountAccessChecker{
                                                obtainer: access::RiotValorantAccountPathObtainer{
                                                    user_id_key: "user_id",
                                                    puuid_key: "puuid",
                                                },
                                            }),
                                        ))
                                        .route("/matches", web::get().to(v1::list_lol_matches_for_user_handler))
                                )
                        )
                )
                .service(
                    web::scope("/tft")
                        .route("", web::post().to(v1::create_tft_match_handler))
                        .service(
                            web::scope("/match/{match_uuid}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::TftMatchAccessChecker{
                                        obtainer: access::TftMatchUuidPathObtainer{
                                            match_uuid_key: "match_uuid",
                                        },
                                    }),
                                ))
                                .route("", web::post().to(v1::finish_tft_match_handler))
                                .route("", web::get().to(v1::get_tft_match_handler))
                                .service(
                                    web::scope("/user/{user_id}")
                                        .route("/vods", web::get().to(v1::get_tft_match_user_accessible_vod_handler))
                                )
                        )
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::DenyShareTokenAccess{}),
                                ))
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
                                ))
                                .route("/backfill", web::post().to(v1::request_tft_match_backfill_handler))
                                .service(
                                    web::scope("/accounts/{puuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::RiotValorantAccountAccessChecker{
                                                obtainer: access::RiotValorantAccountPathObtainer{
                                                    user_id_key: "user_id",
                                                    puuid_key: "puuid",
                                                },
                                            }),
                                        ))
                                        .route("/matches", web::get().to(v1::list_tft_matches_for_user_handler))
                                )
                        )
                )
                .service(
                    web::scope("/valorant")
                        .route("", web::post().to(v1::create_new_valorant_match_handler))
                        .service(
                            // Need to include the user here for us to verify that that the user
                            // is associated with this valorant account.
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::DenyShareTokenAccess{}),
                                ))
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
                                ))
                                .service(
                                    web::scope("/accounts/{puuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::RiotValorantAccountAccessChecker{
                                                obtainer: access::RiotValorantAccountPathObtainer{
                                                    user_id_key: "user_id",
                                                    puuid_key: "puuid",
                                                },
                                            }),
                                        ))
                                        .service(
                                            web::resource("/matches")
                                                .route(web::get().to(v1::list_valorant_matches_for_user_handler))
                                        )
                                        .service(
                                            web::resource("/stats")
                                                .route(web::get().to(v1::get_player_stats_summary_handler))
                                        )
                                )
                                .route("/backfill", web::post().to(v1::request_valorant_match_backfill_handler))
                        )
                        .service(
                            web::scope("/match/{match_uuid}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::ValorantMatchAccessChecker{
                                        obtainer: access::ValorantMatchUuidPathObtainer{
                                            match_uuid_key: "match_uuid",
                                        },
                                    }),
                                ))
                                .service(
                                    web::resource("")
                                        .route(web::get().to(v1::get_valorant_match_details_handler))
                                )
                                .service(
                                    web::resource("/metadata/{puuid}")
                                        .route(web::get().to(v1::get_valorant_player_match_metadata_handler))
                                )
                                .service(
                                    web::scope("/user/{user_id}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::UserSpecificAccessChecker{
                                                obtainer: access::UserIdPathSetObtainer{
                                                    key: "user_id"
                                                },
                                            }),
                                        ))
                                        .route("/vods", web::get().to(v1::get_valorant_match_user_accessible_vod_handler))
                                )
                        )
                )
                .service(
                    web::scope("/aimlab")
                        .route("", web::post().to(v1::create_new_aimlab_task_handler))
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    })
                                ))
                                .route("", web::get().to(v1::list_aimlab_matches_for_user_handler))
                                .service(
                                    web::scope("/match/{match_uuid}")
                                        .service(
                                            web::resource("/task")
                                                .route(web::get().to(v1::get_aimlab_task_data_handler))
                                        )
                                )
                        )
                        .route("/bulk", web::post().to(v1::bulk_create_aimlab_task_handler))
                            .data(web::Json::<Vec<AimlabTask>>::configure(|cfg| {
                                cfg.limit(1 * 1024 * 1024)
                            }))
                )
                .service(
                    web::scope("/hearthstone")
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::UserSpecificAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
                                ).verb_override(
                                    "GET",
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    })
                                ))
                                .service(
                                    web::scope("/match")
                                        .route("", web::post().to(v1::create_hearthstone_match_handler))
                                        .route("", web::get().to(v1::list_hearthstone_matches_for_user_handler))
                                        .service(
                                            web::scope("/{match_uuid}")
                                                .route("", web::post().to(v1::upload_hearthstone_logs_handler))
                                                    .data(web::Payload::configure(|cfg| {
                                                        // Note that we should be submitting GZIP here so this shouldn't get super super large.
                                                        cfg.limit(5 * 1024 * 1024)
                                                    }))
                                                .route("", web::get().to(v1::get_hearthstone_match_handler))
                                                .route("/logs", web::get().to(v1::get_hearthstone_match_logs_handler))
                                                .route("/vods", web::get().to(v1::get_hearthstone_match_user_accessible_vod_handler))
                                        )
                                )
                                .service(
                                    web::scope("/arena")
                                        .route("", web::get().to(v1::list_arena_runs_for_user_handler))
                                        .route("", web::post().to(v1::create_or_retrieve_arena_draft_for_user_handler))
                                        .service(
                                            web::scope("/{collection_uuid}")
                                                .route("", web::post().to(v1::add_hearthstone_card_to_arena_deck_handler))
                                                .route("", web::get().to(v1::get_hearthstone_arena_run_handler))
                                                .route("/matches", web::get().to(v1::list_matches_for_arena_run_handler))
                                                .route("/deck", web::post().to(v1::create_finished_arena_draft_deck_handler))
                                        )
                                )
                                .service(
                                    web::scope("/duels")
                                        .route("", web::get().to(v1::list_duel_runs_for_user_handler))
                                        .service(
                                            web::scope("/{collection_uuid}")
                                                .route("", web::get().to(v1::get_hearthstone_duel_run_handler))
                                                .route("/matches", web::get().to(v1::list_matches_for_duel_run_handler))
                                        )
                                )
                        )
                        .service(
                            web::scope("/cards")
                                .route("", web::post().to(v1::bulk_get_hearthstone_cards_metadata_handler))
                                .route("/battlegrounds/tavern/{tavern_level}", web::get().to(v1::get_battleground_tavern_level_cards_handler))
                        )
                )
                .service(
                    web::scope("/wow")
                        .service(
                            web::scope("/characters")
                                .route("/armory", web::post().to(v1::get_wow_armory_link_for_character_handler))
                        )
                        .service(
                            web::scope("/match")
                                .service(
                                    web::scope("/encounter")
                                        .route("", web::post().to(v1::create_wow_encounter_match_handler))
                                        .service(
                                            web::scope("/{view_uuid}")
                                                .route("", web::post().to(v1::finish_wow_encounter_handler))
                                        )
                                )
                                .service(
                                    web::scope("/challenge")
                                        .route("", web::post().to(v1::create_wow_challenge_match_handler))
                                        .service(
                                            web::scope("/{view_uuid}")
                                                .route("", web::post().to(v1::finish_wow_challenge_handler))
                                        )
                                )
                                .service(
                                    web::scope("/arena")
                                        .route("", web::post().to(v1::create_wow_arena_match_handler))
                                        .service(
                                            web::scope("/{view_uuid}")
                                                .route("", web::post().to(v1::finish_wow_arena_handler))
                                        )
                                )
                                .service(
                                    web::scope("/{match_uuid}")
                                        .service(
                                            web::scope("/users/{user_id}")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::SameSquadAccessChecker{
                                                        obtainer: access::UserIdPathSetObtainer{
                                                            key: "user_id"
                                                        },
                                                    })
                                                ))
                                                .route("/characters", web::get().to(v1::list_wow_characters_association_for_squad_in_match_handler))
                                                .route("/vods", web::get().to(v1::list_wow_vods_for_squad_in_match_handler))
                                        )
                                )
                        )
                        .service(
                            web::scope("/users/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::SameSquadAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    })
                                ))
                                .service(
                                    web::scope("/characters")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("", web::get().to(v1::list_wow_characters_for_user_handler))
                                        .service(
                                            web::scope("/{character_guid}")
                                                .route("/encounters", web::get().to(v1::list_wow_encounters_for_character_handler))
                                                .route("/challenges", web::get().to(v1::list_wow_challenges_for_character_handler))
                                                .route("/arena", web::get().to(v1::list_wow_arenas_for_character_handler))
                                        )
                                )
                                .service(
                                    web::scope("/match/{match_uuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::WowMatchUserMatchupChecker{
                                                obtainer: access::WowMatchUserPathObtainer{
                                                    match_uuid_key: "match_uuid",
                                                    user_id_key: "user_id",
                                                },
                                            })
                                        ))
                                        .route("", web::get().to(v1::get_wow_match_handler))
                                        .route("/characters", web::get().to(v1::list_wow_characters_for_match_handler))
                                        .route("/characters/{character_guid}", web::get().to(v1::get_full_wow_character_for_match_handler))
                                        .route("/events", web::get().to(v1::list_wow_events_for_match_handler))
                                        .route("/death/{event_id}", web::get().to(v1::get_death_recap_handler))
                                        .service(
                                            web::scope("/stats")
                                                .route("/dps", web::get().to(v1::get_wow_match_dps_handler))
                                                .route("/hps", web::get().to(v1::get_wow_match_heals_per_second_handler))
                                                .route("/drps", web::get().to(v1::get_wow_match_damage_received_per_second_handler))
                                        )
                                )
                        )
                )
                .service(
                    web::scope("/vod")
                        .route("", web::post().to(v1::create_vod_destination_handler))
                        .route("/bulkDelete", web::post().to(v1::bulk_delete_vods_handler))
                        .service(
                            web::scope("/match/{match_uuid}")
                                .service(
                                    web::scope("/user")
                                        .service(
                                            web::resource("/id/{user_id}")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::SameSquadAccessChecker{
                                                        obtainer: access::UserIdPathSetObtainer{
                                                            key: "user_id"
                                                        },
                                                    })
                                                ))
                                                .route(web::get().to(v1::find_vod_from_match_user_id_handler))
                                        )
                                )
                        )
                        .service(
                            web::scope("/{video_uuid}")
                                .service(
                                    web::scope("/list")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: false,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ))
                                        .route("/favorite", web::post().to(v1::favorite_vod_handler))
                                        .route("/favorite", web::delete().to(v1::remove_favorite_vod_handler))
                                        .route("/favorite", web::get().to(v1::check_favorite_vod_handler))
                                        .route("/watch", web::post().to(v1::watchlist_vod_handler))
                                        .route("/watch", web::delete().to(v1::remove_watchlist_vod_handler))
                                        .route("/watch", web::get().to(v1::check_watchlist_vod_handler))
                                )
                                .service(
                                    web::resource("/{quality}/{segment_name}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: false,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ))
                                        .route(web::get().to(v1::get_vod_track_segment_handler))
                                )
                                .service(
                                    web::resource("/clip")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: false,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ))
                                        .route(web::post().to(v1::create_clip_for_vod_handler))
                                )
                                .service(
                                    web::scope("")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: true,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ).verb_override(
                                            "GET",
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: false,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ))
                                        .route("", web::delete().to(v1::delete_vod_handler))
                                        .route("", web::get().to(v1::get_vod_handler))
                                        .route("", web::post().to(v1::associate_vod_handler))
                                        .route("/assoc", web::get().to(v1::get_vod_association_handler))
                                        .route("/upload", web::get().to(v1::get_vod_upload_path_handler))
                                )
                        )
                )
                .service(
                    web::scope("/clip")
                        .route("", web::get().to(v1::list_clips_for_user_handler))
                        .route("/bulkDelete", web::post().to(v1::bulk_delete_vods_handler))
                        .service(
                            web::scope("/{clip_uuid}")
                                .route("", web::get().to(v1::get_clip_handler))
                                .service(
                                    web::scope("/share")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("", web::post().to(v1::create_clip_share_signature_handler))
                                )
                                .service(
                                    web::scope("/react")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("", web::get().to(v1::get_clip_reacts_handler))
                                        .route("", web::post().to(v1::add_react_to_clip_handler))
                                        .route("", web::delete().to(v1::delete_react_from_clip_handler))
                                )
                                .service(
                                    web::scope("/view")
                                        .route("", web::post().to(v1::mark_clip_view_handler))
                                )
                                .service(
                                    web::scope("/comments")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("", web::get().to(v1::get_clip_comments_handler))
                                        .route("", web::post().to(v1::create_clip_comment_handler))
                                        .service(
                                            web::scope("/{comment_id}")
                                                .route("", web::delete().to(v1::delete_clip_comment_handler))
                                        )
                                )
                        )
                )
                .service(
                    web::scope("/squad")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("", web::post().to(v1::create_squad_handler))
                        .service(
                            web::scope("/{squad_id}")
                                // Owner-only endpoints
                                .service(
                                    web::scope("/admin")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::SquadAccessChecker{
                                                requires_owner: true,
                                                obtainer: access::SquadIdPathSetObtainer{
                                                    key: "squad_id"
                                                },
                                            }),
                                        ))
                                        .route("", web::delete().to(v1::delete_squad_handler))
                                        .route("", web::put().to(v1::edit_squad_handler))
                                        .route("/invite/{invite_uuid}/revoke", web::post().to(v1::revoke_squad_invite_handler))
                                        .route("/membership/{user_id}", web::delete().to(v1::kick_squad_member_handler))
                                )
                                .service(
                                    web::scope("/invite/{invite_uuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::UserSpecificAccessChecker{
                                                obtainer: access::SquadInvitePathObtainer{
                                                    key: "invite_uuid"
                                                },
                                            }),
                                        ))
                                        .route("/accept", web::post().to(v1::accept_squad_invite_handler))
                                        .route("/reject", web::post().to(v1::reject_squad_invite_handler))
                                )
                                // Metadata about the squad should be public (without access checks besides being logged in
                                // so that people can know what squads they're being invited to.
                                .route("/profile", web::get().to(v1::get_squad_handler))
                                // Member-only endpoints
                                .service(
                                    web::scope("")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::SquadAccessChecker{
                                                requires_owner: false,
                                                obtainer: access::SquadIdPathSetObtainer{
                                                    key: "squad_id"
                                                },
                                            }),
                                        ))
                                        .route("/leave", web::post().to(v1::leave_squad_handler))
                                        .service(
                                            web::scope("/invite")
                                                .route("", web::post().to(v1::create_squad_invite_handler))
                                                .route("", web::get().to(v1::get_all_squad_invites_handler))
                                        )
                                        .service(
                                            web::scope("/membership")
                                                .route("/{user_id}", web::get().to(v1::get_squad_user_membership_handler))
                                                .route("", web::get().to(v1::get_all_squad_user_memberships_handler))
                                        )
                                )
                        )
                )
        );

    if graphql_debug {
        scope = scope.service(
            web::resource("/graphql")
                .route(web::post().to(graphql::graphql_handler))
                .route(web::get().to(graphql::graphiql_handler))
        );
    } else {
        scope = scope.service(
            web::resource("/graphql")
                .wrap(auth::ApiSessionValidator{})
                .route(web::post().to(graphql::graphql_handler))
        );
    }
    scope
}