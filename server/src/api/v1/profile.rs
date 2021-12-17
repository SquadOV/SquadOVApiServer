use crate::api::{
    ApiApplication,
    auth::SquadOVSession,
    v1::VodFindFromVideoUuid,
};
use actix_web::{
    web::{
        self,
        BufMut,
    },
    HttpResponse,
    HttpRequest,
};
use squadov_common::{
    SquadOvError,
    profile::{
        self,
        data::UserProfileBasicUpdateData,
        access::UserProfileBasicUpdateAccess,
        UserProfileBasicSerialized,
    },
    image,
    storage::CloudStorageLocation,
    vod::db as vdb,
};
use serde::{Serialize, Deserialize};
use serde_qs::actix::QsQuery;
use std::sync::Arc;
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct UserProfileQuery {
    id: Option<i64>,
    slug: Option<String>,
}

#[derive(Deserialize)]
pub struct UserProfilePath {
    pub profile_id: i64,
}

#[derive(Deserialize)]
pub struct UserProfileSlugInput {
    pub slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProfileMatchClipInput {
    pub match_uuid: Option<Uuid>,
    pub clip_uuid: Option<Uuid>,
}

impl ApiApplication {
    // Processes and stores the image and returns the blob UUID that we can retrieve the image from.
    async fn process_and_store_profile_image(&self, data: &[u8]) -> Result<Uuid, SquadOvError> {
        let data = image::process_raw_image_buffer_into_standard_jpeg(data)?;
        
        let bucket = self.blob.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global location for blob storage.")))?;
        let manager = self.get_blob_manager(&bucket).await?;

        let mut tx = self.pool.begin().await?;
        let uuid = manager.store_new_blob(&mut tx, &data, false).await?;
        tx.commit().await?;
        Ok(uuid)
    }

    async fn get_user_profile_from_query(&self, request_user_id: Option<i64>, query: UserProfileQuery) -> Result<Option<UserProfileBasicSerialized>, SquadOvError> {
        let raw_profile = if let Some(id) = query.id {
            profile::get_user_profile_from_id(&*self.pool, id).await?
        } else if let Some(slug) = &query.slug {
            profile::get_user_profile_from_slug(&*self.pool, &slug).await?
        } else {
            return Err(SquadOvError::BadRequest);
        };
    
        Ok(if let Some(raw_profile) = raw_profile {
            let bucket = self.blob.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global location for blob storage.")))?;
            let manager = self.get_blob_manager(&bucket).await?;
            Some(profile::get_user_profile_basic_serialized_with_requester(&*self.pool, raw_profile, request_user_id, manager, &self.config.squadov.access_key).await?)
        } else {
            None
        })
    }
}

pub async fn get_basic_profile_handler(app : web::Data<Arc<ApiApplication>>, query: QsQuery<UserProfileQuery>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    // Need to determine WHO is making this request.
    // Is it public? Or is there an actual session behind this.
    let extensions = req.extensions();
    let request_user_id = extensions.get::<SquadOVSession>().map(|x| { x.user.id });
    Ok(HttpResponse::Ok().json(app.get_user_profile_from_query(request_user_id, query.into_inner()).await?))
}

pub async fn edit_current_user_profile_basic_data_handler(app : web::Data<Arc<ApiApplication>>, mut payload: Multipart, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    
    let mut cover_photo = None;
    let mut profile_photo = None;
    let mut basic_data = UserProfileBasicUpdateData::default();
    
    while let Some(mut field) = payload.try_next().await? {
        let content_type = field.content_disposition().ok_or(SquadOvError::BadRequest)?;
        let field_name = content_type.get_name().ok_or(SquadOvError::BadRequest)?;
        
        let mut tmp = web::BytesMut::new();
        while let Some(Ok(chunk)) = field.next().await {
            tmp.put(&*chunk);
        }

        match field_name {
            "coverPhoto" => { cover_photo = Some(tmp) },
            "profilePhoto" => { profile_photo = Some(tmp) },
            "description" => { basic_data.description = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "facebook" => { basic_data.facebook = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "instagram" => { basic_data.instagram = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "twitch" => { basic_data.twitch = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "youtube" => { basic_data.youtube = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "snapchat" => { basic_data.snapchat = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "twitter" => { basic_data.twitter = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "tiktok" => { basic_data.tiktok = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            "displayName" => { basic_data.display_name = Some(std::str::from_utf8(&*tmp)?.to_string()) },
            _ => log::warn!("Unknown field name for profile: {}", field_name),
        }
    }

    // Process cover photo and profile photo and then upload; after we upload, get the URL to store for access.
    let cover_photo_blob: Option<Uuid> = if let Some(cover_photo_data) = cover_photo {
        Some(app.process_and_store_profile_image(&cover_photo_data).await?)
    } else {
        None
    };
    let profile_photo_blob: Option<Uuid> = if let Some(profile_photo_data) = profile_photo {
        Some(app.process_and_store_profile_image(&profile_photo_data).await?)
    } else {
        None
    };

    // Update the database with information with all this new data.
    let mut tx = app.pool.begin().await?;

    // We need to do 3 separate database updates here:
    //  1) Cover Photo
    //  2) Profile Photo
    //  3) Display Name + Misc (Description + Links)
    // Primarily because a "None" for cover/profile photos doesn't mean
    // we want to delete, it just means nothing was uploaded in this update
    // so we need logic to take of that difference in what a "None" is. In the
    // case of the other fields, a none means that we actually want to clear
    // that field in the database.
    if cover_photo_blob.is_some() {
        profile::data::update_user_profile_cover_photo_blob(&mut tx, session.user.id, cover_photo_blob.as_ref()).await?;
    }

    if profile_photo_blob.is_some() {
        profile::data::update_user_profile_profile_photo_blob(&mut tx, session.user.id, profile_photo_blob.as_ref()).await?;
    }

    profile::data::update_user_profile_basic_data(&mut tx, session.user.id, &basic_data).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn edit_current_user_profile_basic_access_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<UserProfileBasicUpdateAccess>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let mut tx = app.pool.begin().await?;
    profile::access::update_user_profile_basic_access(&mut tx, session.user.id, &data).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn create_user_profile_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<UserProfileSlugInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    profile::create_user_profile_for_user_id(&*app.pool, session.user.id, &data.slug).await?;

    Ok(HttpResponse::Ok().json(app.get_user_profile_from_query(Some(session.user.id), UserProfileQuery{
        id: Some(session.user.id),
        slug: None,
    }).await?))
}

pub async fn get_match_clip_profile_share_handler(app : web::Data<Arc<ApiApplication>>, data: web::Query<ProfileMatchClipInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    #[derive(Serialize)]
    #[serde(rename_all="camelCase")]
    pub struct SharedResponse {
        can_share: bool,
        is_shared: bool,
    }

    let response = if let Some(match_uuid) = data.match_uuid.as_ref() {
        SharedResponse{
            can_share: vdb::check_user_has_recorded_vod_for_match(&*app.pool, session.user.id, match_uuid).await?,
            is_shared: profile::data::check_if_match_shared_to_profile(&*app.pool, session.user.id, match_uuid).await?,
        }
    } else if let Some(clip_uuid) = data.clip_uuid.as_ref() {
        SharedResponse{
            can_share: vdb::check_user_is_vod_owner(&*app.pool, session.user.id, clip_uuid).await?,
            is_shared: profile::data::check_if_clip_shared_to_profile(&*app.pool, session.user.id, clip_uuid).await?,
        }
    } else {
        return Err(SquadOvError::BadRequest);
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn create_match_clip_profile_share_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<ProfileMatchClipInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    if let Some(match_uuid) = data.match_uuid.as_ref() {
        profile::data::add_match_to_user_profile(&*app.pool, match_uuid, session.user.id).await?;
    } else if let Some(clip_uuid) = data.clip_uuid.as_ref() {
        profile::data::add_clip_to_user_profile(&*app.pool, clip_uuid, session.user.id).await?;
    } else {
        return Err(SquadOvError::BadRequest);
    }

    Ok(HttpResponse::NoContent().finish())
}

pub async fn delete_match_clip_profile_share_handler(app : web::Data<Arc<ApiApplication>>, data: web::Query<ProfileMatchClipInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    
    if let Some(match_uuid) = data.match_uuid.as_ref() {
        profile::data::remove_match_from_user_profile(&*app.pool, match_uuid, session.user.id).await?;
    } else if let Some(clip_uuid) = data.clip_uuid.as_ref() {
        profile::data::remove_clip_from_user_profile(&*app.pool, clip_uuid, session.user.id).await?;
    } else {
        return Err(SquadOvError::BadRequest);
    }

    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_profile_info_for_vod_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<VodFindFromVideoUuid>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(&profile::get_user_profile_handle_from_video_uuid(&*app.pool, &path.video_uuid).await?))
}