use crate::{
    Arcadia,
    models::{artist::UserCreatedArtist, title_group::UserCreatedAffiliatedArtist, user::User},
    repositories::artist_repository::{
        create_artist, create_artists_affiliation, find_artist_publications,
    },
};
use actix_web::{HttpResponse, web};
use serde::Deserialize;

pub async fn add_artist(
    artist: web::Json<UserCreatedArtist>,
    arc: web::Data<Arcadia>,
    current_user: User,
) -> HttpResponse {
    match create_artist(&arc.pool, &artist, &current_user).await {
        Ok(created_artist) => HttpResponse::Created().json(serde_json::json!(created_artist)),
        Err(err) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": err.to_string()
        })),
    }
}

pub async fn add_affiliated_artists(
    artists: web::Json<Vec<UserCreatedAffiliatedArtist>>,
    arc: web::Data<Arcadia>,
    current_user: User,
) -> HttpResponse {
    match create_artists_affiliation(&arc.pool, &artists, &current_user).await {
        Ok(created_affiliations) => {
            HttpResponse::Created().json(serde_json::json!(created_affiliations))
        }
        Err(err) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": err.to_string()
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct GetArtistPublicationsQuery {
    id: i32,
}

pub async fn get_artist_publications(
    query: web::Query<GetArtistPublicationsQuery>,
    arc: web::Data<Arcadia>,
) -> HttpResponse {
    match find_artist_publications(&arc.pool, &query.id).await {
        Ok(artist_publications) => {
            HttpResponse::Created().json(serde_json::json!(artist_publications))
        }
        Err(err) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": err.to_string()
        })),
    }
}
