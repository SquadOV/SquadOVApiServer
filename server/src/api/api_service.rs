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
            web::scope("/twitch")
                .route("/eventsub", web::post().to(v1::on_twitch_eventsub_handler))
        )
        .service(
            web::scope("/admin")
                .wrap(access::ApiAccess::new(
                    Box::new(access::ShareTokenAccessRestricter{}),
                ))
                .wrap(access::ApiAccess::new(
                    Box::new(access::AdminAccessChecker{}),
                ))
                .wrap(auth::ApiSessionValidator{required: true})
                .service(
                    web::scope("/analytics")
                        .route("/daily", web::get().to(admin::get_daily_analytics_handler))
                        .route("/monthly", web::get().to(admin::get_monthly_analytics_handler))
                )
        )
        .service(
            web::scope("/auth")
                .route("/login", web::post().to(auth::login_handler))
                .route("/login/mfa", web::post().to(auth::mfa_login_handler))
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
                        .route("/twitch", web::post().to(v1::handle_twitch_oauth_callback_handler))
                )
                .service(
                    // These are the only two endpoints where the user needs to provide a valid session to use.
                    web::scope("")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .wrap(auth::ApiSessionValidator{required: true})
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
                )
                .service(
                    web::scope("/flags")
                        .route("", web::get().to(v1::get_global_app_flags_handler))
                )
                .service(
                    web::scope("/community/slug/{community_slug}")
                        .route("", web::get().to(v1::get_community_slug_handler))
                )
                .service(
                    web::scope("/link/{link_id}")
                        .route("", web::get().to(v1::get_public_invite_link_data_handler))
                )
        )
        .service(
            web::scope("/profile")
                .wrap(auth::ApiSessionValidator{required: false})
                .route("", web::get().to(v1::get_basic_profile_handler))
                .service(
                    web::scope("/{profile_id}")
                        .wrap(access::ApiAccessToken::new())
                        .service(
                            web::scope("/matches")
                                .route("", web::post().to(v1::get_profile_matches_handler))
                        )
                        .service(
                            web::scope("/clips")
                                .route("", web::post().to(v1::get_profile_clips_handler))
                        )
                )
        )
        .service(
            web::scope("/v1")
                .wrap(access::ApiAccessToken::new().make_optional())
                .wrap(access::ApiAccess::new(
                    Box::new(access::ShareTokenAccessRestricter{}),
                ))
                .wrap(auth::ApiSessionValidator{required: true})
                .service(
                    web::scope("/link")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .service(
                            web::scope("/{link_id}")
                                .route("/accept", web::post().to(v1::use_link_to_join_squad_handler))
                        )
                )
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
                    web::scope("/sentry")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("/desktop", web::get().to(v1::get_desktop_sentry_info_handler))
                        .route("/web", web::get().to(v1::get_web_sentry_info_handler))
                )
                .service(
                    web::scope("/share")
                        .route("", web::post().to(v1::create_new_share_connection_handler))
                        .route("/permissions", web::post().to(v1::get_share_permissions_handler))
                        .service(
                            web::scope("/conn/{connection_id}")
                                .route("", web::delete().to(v1::delete_share_connection_handler))
                                .route("", web::post().to(v1::edit_share_connection_handler))
                        )
                        .service(
                            web::scope("/auto")
                                .route("", web::get().to(v1::get_auto_share_settings_handler))
                                .route("", web::post().to(v1::new_auto_share_setting_handler))
                                .service(
                                    web::scope("/{setting_id}")
                                        .route("", web::delete().to(v1::delete_auto_share_setting_handler))
                                        .route("", web::post().to(v1::edit_auto_share_setting_handler))
                                )
                        )
                        .service(
                            web::resource("/profile")
                                .route(web::get().to(v1::get_match_clip_profile_share_handler))
                                .route(web::post().to(v1::create_match_clip_profile_share_handler))
                                .route(web::delete().to(v1::delete_match_clip_profile_share_handler))
                        )
                        .service(
                            web::resource("/settings")
                                .route(web::get().to(v1::get_user_auto_share_settings_handler))
                                .route(web::post().to(v1::edit_user_auto_share_settings_handler))
                        )
                )
                .service(
                    web::scope("/match/{match_uuid}")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("/share/internal", web::get().to(v1::get_match_share_connections_handler))
                        .route("/share/public", web::delete().to(v1::delete_match_share_link_handler))
                        .route("/share/public", web::get().to(v1::get_match_share_link_handler))
                        .route("/share/public", web::post().to(v1::create_match_share_signature_handler))
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
                                    web::scope("/profile")
                                        .route("", web::get().to(v1::get_current_user_profile_handler))
                                        .route("", web::post().to(v1::create_user_profile_handler))
                                        .route("/username", web::post().to(v1::edit_current_user_username_handler))
                                        .route("/email", web::post().to(v1::edit_current_user_email_handler))
                                        .route("/data", web::post().to(v1::edit_current_user_profile_basic_data_handler))
                                            .data(web::Payload::configure(|cfg| {
                                                // Technically only need 6MB (5MB cover, 1MB profile)
                                                // but giving us some wiggle room.
                                                cfg.limit(10 * 1024 * 1024)
                                            }))
                                        .route("/access", web::post().to(v1::edit_current_user_profile_basic_access_handler))
                                )
                                .service(
                                    web::resource("/notifications")
                                        .route(web::get().to(v1::get_current_user_notifications_handler))
                                )
                                .route("/active", web::post().to(v1::mark_user_active_endpoint_handler))
                                .route("/download", web::post().to(v1::mark_user_download_handler))
                                .route("/playtime", web::get().to(v1::get_user_recorded_playtime_handler))
                                .route("/recent", web::post().to(v1::get_recent_matches_for_me_handler))
                                .route("/referral", web::get().to(v1::get_user_me_referral_link_handler))
                                .route("/squadmates", web::get().to(v1::get_user_squadmates_handler))
                                .route("/changepw", web::post().to(auth::change_pw_handler))
                                .route("/pw/verify", web::post().to(auth::verify_pw_handler))
                                .route("/hw", web::post().to(v1::sync_user_hardware_handler))
                                .route("/identify", web::post().to(v1::perform_user_analytics_identify_handler))
                                .service(
                                    web::scope("/2fa")
                                        .route("/qr", web::get().to(auth::get_2fa_qr_code_handler))
                                        .route("", web::get().to(auth::check_2fa_status_handler))
                                        .route("", web::delete().to(auth::remove_2fa_handler))
                                        .route("", web::post().to(auth::enable_2fa_handler))
                                )
                                .service(
                                    web::scope("/accounts")
                                        .route("", web::get().to(v1::get_all_my_linked_accounts_handler))
                                        .service(
                                            web::scope("/twitch")
                                                .route("", web::get().to(v1::get_my_linked_twitch_account_handler))
                                        )
                                )
                                .service(
                                    web::scope("/oauth")
                                        .route("/twitch", web::get().to(v1::get_twitch_login_url_handler))
                                )
                                .service(
                                    web::scope("/discover")
                                        .route("/squads", web::get().to(v1::get_user_discover_squads_handler))
                                )
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
                                        .service(
                                            web::scope("/{squad_id}")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::SquadAccessChecker{
                                                        requires_owner: false,
                                                        obtainer: access::SquadIdPathSetObtainer{
                                                            key: "squad_id"
                                                        },
                                                    }),
                                                ))
                                                .service(
                                                    web::scope("/links")
                                                        .route("", web::get().to(v1::get_user_squad_invite_links_handler))
                                                        .route("", web::post().to(v1::create_user_squad_invite_link_handler))
                                                        .service(
                                                            web::scope("/{link_id}")
                                                                .route("", web::put().to(v1::edit_user_squad_invite_link_handler))
                                                                .route("", web::delete().to(v1::delete_user_squad_invite_link_handler))
                                                        )
                                                )
                                        )
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
                                .route("/finish", web::post().to(v1::finish_lol_match_handler))
                                .route("/vods", web::get().to(v1::get_lol_match_user_accessible_vod_handler))
                                .service(
                                    web::scope("")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::LolMatchAccessChecker{
                                                obtainer: access::LolMatchUuidPathObtainer{
                                                    match_uuid_key: "match_uuid",
                                                },
                                            }),
                                        ))
                                        .route("", web::get().to(v1::get_lol_match_handler))
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
                                .route("/finish", web::post().to(v1::finish_tft_match_handler))
                                .route("/vods", web::get().to(v1::get_tft_match_user_accessible_vod_handler))
                                .service(
                                    web::scope("")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::TftMatchAccessChecker{
                                                obtainer: access::TftMatchUuidPathObtainer{
                                                    match_uuid_key: "match_uuid",
                                                },
                                            }),
                                        ))
                                        .route("", web::get().to(v1::get_tft_match_handler))
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
                                .route("/vods", web::get().to(v1::get_valorant_match_user_accessible_vod_handler))
                        )
                )
                .service(
                    web::scope("/aimlab")
                        .route("", web::post().to(v1::create_new_aimlab_task_handler))
                        .service(
                            web::scope("/user/{user_id}")
                                .route("", web::get().to(v1::list_aimlab_matches_for_user_handler))
                                .service(
                                    web::scope("/match/{match_uuid}")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::AimlabMatchUserMatchupChecker{
                                                obtainer: access::AimlabMatchUserPathObtainer{
                                                    match_uuid_key: "match_uuid",
                                                    user_id_key: "user_id",
                                                },
                                            })
                                        ))
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::MatchVodAccessChecker{
                                                obtainer: access::MatchVodPathObtainer{
                                                    match_key: Some("match_uuid"),
                                                    video_key: None,
                                                    user_key: Some("user_id"),
                                                },
                                            })
                                        ))
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
                        .route("/match/{match_uuid}/vods", web::get().to(v1::get_hearthstone_match_user_accessible_vod_handler))
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::NullUserSetAccessChecker{})
                                ).verb_override(
                                    "POST",
                                    Box::new(access::UserSpecificAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
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
                                                .service(
                                                    web::scope("")
                                                        .wrap(access::ApiAccess::new(
                                                            Box::new(access::HearthstoneMatchUserMatchupChecker{
                                                                obtainer: access::HearthstoneMatchUserPathObtainer{
                                                                    match_uuid_key: "match_uuid",
                                                                    user_id_key: "user_id",
                                                                },
                                                            })
                                                        ))
                                                        .wrap(access::ApiAccess::new(
                                                            Box::new(access::MatchVodAccessChecker{
                                                                obtainer: access::MatchVodPathObtainer{
                                                                    match_key: Some("match_uuid"),
                                                                    video_key: None,
                                                                    user_key: Some("user_id"),
                                                                },
                                                            })
                                                        ))
                                                        .route("", web::get().to(v1::get_hearthstone_match_handler))
                                                        .route("/logs", web::get().to(v1::get_hearthstone_match_logs_handler))
                                                )
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
                                .route("/armory", web::get().to(v1::get_wow_armory_link_for_character_handler))
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
                                    web::scope("/instance")
                                        .route("", web::post().to(v1::create_wow_instance_match_handler))
                                        .service(
                                            web::scope("/{view_uuid}")
                                                .route("", web::post().to(v1::finish_wow_instance_handler))
                                                .service(
                                                    web::scope("/convert")
                                                        .route("/keystone", web::post().to(v1::convert_wow_instance_to_keystone_handler))
                                                )
                                        )
                                )
                                .service(
                                    web::scope("/{match_uuid}")
                                        .route("/vods", web::get().to(v1::list_wow_vods_for_squad_in_match_handler))
                                        .service(
                                            web::scope("/users/{user_id}")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::MatchVodAccessChecker{
                                                        obtainer: access::MatchVodPathObtainer{
                                                            match_key: Some("match_uuid"),
                                                            video_key: None,
                                                            user_key: Some("user_id"),
                                                        },
                                                    })
                                                ))
                                                .route("/characters", web::get().to(v1::list_wow_characters_association_for_squad_in_match_handler))
                                        )
                                )
                        )
                        .service(
                            web::scope("/users/{user_id}")
                                .service(
                                    web::scope("/characters")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::SameSquadAccessChecker{
                                                obtainer: access::UserIdPathSetObtainer{
                                                    key: "user_id"
                                                },
                                            })
                                        ))
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("", web::get().to(v1::list_wow_characters_for_user_handler))
                                        .service(
                                            web::scope("/{character_guid}")
                                                .route("/encounters", web::get().to(v1::list_wow_encounters_for_character_handler))
                                                .route("/challenges", web::get().to(v1::list_wow_challenges_for_character_handler))
                                                .route("/arena", web::get().to(v1::list_wow_arenas_for_character_handler))
                                                .route("/instance", web::get().to(v1::list_wow_instances_for_character_handler))
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
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::MatchVodAccessChecker{
                                                obtainer: access::MatchVodPathObtainer{
                                                    match_key: Some("match_uuid"),
                                                    video_key: None,
                                                    user_key: Some("user_id"),
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
                    web::scope("/csgo")
                        .route("/match/{match_uuid}/vods", web::get().to(v1::get_csgo_match_accessible_vods_handler))
                        .service(
                            web::scope("/user/{user_id}")
                                .wrap(access::ApiAccess::new(
                                    Box::new(access::NullUserSetAccessChecker{})
                                ).verb_override(
                                    "POST",
                                    Box::new(access::UserSpecificAccessChecker{
                                        obtainer: access::UserIdPathSetObtainer{
                                            key: "user_id"
                                        },
                                    }),
                                ))
                                .service(
                                    web::scope("/view")
                                        .route("", web::post().to(v1::create_csgo_view_for_user_handler))
                                        .service(
                                            web::scope("/{view_uuid}")
                                                .route("", web::post().to(v1::finish_csgo_view_for_user_handler))
                                                    .data(web::Payload::configure(|cfg| {
                                                        // Note that we should be submitting GZIP here so this shouldn't get super super large.
                                                        cfg.limit(5 * 1024 * 1024)
                                                    }))
                                                .route("/demo", web::post().to(v1::associate_csgo_demo_with_view_handler))
                                        )
                                )
                                .service(
                                    web::scope("/match")
                                        .route("", web::get().to(v1::list_csgo_matches_for_user_handler))
                                        .service(
                                            web::scope("/{match_uuid}")
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::CsgoMatchUserMatchupChecker{
                                                        obtainer: access::CsgoMatchUserPathObtainer{
                                                            match_uuid_key: "match_uuid",
                                                            user_id_key: "user_id",
                                                        },
                                                    })
                                                ))
                                                .wrap(access::ApiAccess::new(
                                                    Box::new(access::MatchVodAccessChecker{
                                                        obtainer: access::MatchVodPathObtainer{
                                                            match_key: Some("match_uuid"),
                                                            video_key: None,
                                                            user_key: Some("user_id"),
                                                        },
                                                    })
                                                ))
                                                .route("", web::get().to(v1::get_csgo_match_handler))
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
                                                    Box::new(access::MatchVodAccessChecker{
                                                        obtainer: access::MatchVodPathObtainer{
                                                            match_key: Some("match_uuid"),
                                                            video_key: None,
                                                            user_key: Some("user_id"),
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
                                    web::scope("/tag")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::VodAccessChecker{
                                                must_be_vod_owner: false,
                                                obtainer: access::VodPathObtainer{
                                                    video_uuid_key: "video_uuid"
                                                },
                                            }),
                                        ))
                                        .route("", web::get().to(v1::get_tags_for_vod_handler))
                                        .route("", web::post().to(v1::add_tags_for_vod_handler))
                                        .route("/{tag_id}", web::delete().to(v1::delete_tag_for_vod_handler))
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
                                        .route("/fastify", web::get().to(v1::get_vod_fastify_status_handler))
                                        .route("/upload", web::get().to(v1::get_vod_upload_path_handler))
                                        .route("/match", web::get().to(v1::get_vod_recent_match_handler))
                                )
                        )
                )
                .service(
                    web::scope("/clip")
                        .route("", web::post().to(v1::list_clips_for_user_handler))
                        .route("/bulkDelete", web::post().to(v1::bulk_delete_vods_handler))
                        .service(
                            web::scope("/{clip_uuid}")
                                .service(
                                    web::scope("/share")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::DenyShareTokenAccess{}),
                                        ))
                                        .route("/public", web::post().to(v1::create_clip_share_signature_handler))
                                        .route("/public", web::get().to(v1::get_clip_share_signature_handler))
                                        .route("/public", web::delete().to(v1::delete_clip_share_signature_handler))
                                        .route("/internal", web::get().to(v1::get_clip_share_connections_handler))
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
                                        .route("", web::get().to(v1::mark_clip_view_handler))
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
                                .service(
                                    web::scope("")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::MatchVodAccessChecker{
                                                obtainer: access::MatchVodPathObtainer{
                                                    match_key: None,
                                                    video_key: Some("clip_uuid"),
                                                    user_key: None,
                                                },
                                            })
                                        ))
                                        .route("", web::get().to(v1::get_clip_handler))
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
                                        .service(
                                            web::scope("/membership/{user_id}")
                                                .route("", web::delete().to(v1::kick_squad_member_handler))
                                                .route("/share", web::post().to(v1::change_squad_member_can_share_handler))
                                        )
                                        .route("/share", web::post().to(v1::update_squad_share_settings_handler))
                                        .route("/content/{video_uuid}", web::delete().to(v1::remove_content_from_squad_handler))
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
                                .route("/join", web::post().to(v1::join_public_squad_handler))
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
                                        .route("/share", web::get().to(v1::get_squad_share_settings_handler))
                                )
                        )
                )
                .service(
                    web::scope("/community")
                        .wrap(access::ApiAccess::new(
                            Box::new(access::DenyShareTokenAccess{}),
                        ))
                        .route("", web::post().to(v1::create_community_handler))
                        .route("", web::get().to(v1::list_communities_handler))
                        .service(
                            web::scope("/slug/{community_slug}")
                                .route("/role", web::get().to(v1::get_community_role_handler))
                                .route("/sub", web::get().to(v1::get_community_sub_handler))
                        )
                        .service(
                            web::scope("/id/{community_id}")
                                .route("", web::get().to(v1::get_community_handler))
                                .route("/join", web::post().to(v1::join_community_handler))
                                .route("/leave", web::post().to(v1::leave_community_handler))
                                .service(
                                    web::scope("/owner")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::CommunityAccessChecker{
                                                obtainer: access::CommunityIdPathSetObtainer{
                                                    key: "community_id"
                                                },
                                                is_owner: true,
                                                can_manage: false,
                                                can_moderate: false,
                                                can_invite: false,
                                                can_share: false,
                                            }),
                                        ))
                                        .route("", web::delete().to(v1::delete_community_handler))
                                        .route("", web::post().to(v1::edit_community_handler))
                                )
                                .service(
                                    web::scope("/manage")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::CommunityAccessChecker{
                                                obtainer: access::CommunityIdPathSetObtainer{
                                                    key: "community_id"
                                                },
                                                is_owner: false,
                                                can_manage: true,
                                                can_moderate: false,
                                                can_invite: false,
                                                can_share: false,
                                            }),
                                        ))
                                        .service(
                                            web::scope("/users")
                                                .route("", web::get().to(v1::list_users_in_community_handler))
                                                .service(
                                                    web::scope("/{user_id}")
                                                        .route("", web::delete().to(v1::remove_user_from_community_handler))
                                                        .route("", web::post().to(v1::edit_user_in_community_handler))
                                                )
                                        )
                                        .service(
                                            web::scope("/roles")
                                                .route("", web::get().to(v1::list_roles_in_community_handler))
                                                .route("", web::post().to(v1::create_role_in_community_handler))
                                                .service(
                                                    web::scope("/{role_id}")
                                                        .route("", web::delete().to(v1::remove_role_from_community_handler))
                                                        .route("", web::post().to(v1::edit_role_in_community_handler))
                                                )
                                        )
                                )
                                .service(
                                    web::scope("/moderate")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::CommunityAccessChecker{
                                                obtainer: access::CommunityIdPathSetObtainer{
                                                    key: "community_id"
                                                },
                                                is_owner: false,
                                                can_manage: false,
                                                can_moderate: true,
                                                can_invite: false,
                                                can_share: false,
                                            }),
                                        ))
                                )
                                .service(
                                    web::scope("/invite")
                                        .wrap(access::ApiAccess::new(
                                            Box::new(access::CommunityAccessChecker{
                                                obtainer: access::CommunityIdPathSetObtainer{
                                                    key: "community_id"
                                                },
                                                is_owner: false,
                                                can_manage: false,
                                                can_moderate: false,
                                                can_invite: true,
                                                can_share: false,
                                            }),
                                        ))
                                        .route("", web::post().to(v1::create_community_invite_handler))
                                        .route("", web::get().to(v1::get_community_invites_handler))
                                        .service(
                                            web::scope("/{code}")
                                                .route("", web::delete().to(v1::delete_community_invite_handler))
                                        )
                                )
                        )                        
                )
        );

    if graphql_debug {
        scope = scope.service(
            web::resource("/graphql")
                .wrap(auth::ApiSessionValidator{required: true})
                .route(web::post().to(graphql::graphql_handler))
                .route(web::get().to(graphql::graphiql_handler))
        );
    } else {
        scope = scope.service(
            web::resource("/graphql")
                .wrap(auth::ApiSessionValidator{required: true})
                .route(web::post().to(graphql::graphql_handler))
        );
    }
    scope
}