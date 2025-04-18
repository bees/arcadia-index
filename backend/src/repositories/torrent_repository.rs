use crate::{
    Error, Result,
    models::{
        title_group::TitleGroupHierarchyLite,
        torrent::{Features, Torrent, TorrentSearch, UploadedTorrent},
        user::User,
    },
};

use bip_metainfo::Metainfo;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, map::VacantEntry};
use sqlx::{PgPool, prelude::FromRow};
use std::{fs, path::PathBuf, str::FromStr};

use super::notification_repository::notify_users;

#[derive(sqlx::FromRow)]
struct TitleGroupInfoLite {
    id: i64,
    name: String,
}

pub async fn create_torrent(
    pool: &PgPool,
    torrent_form: &UploadedTorrent,
    current_user: &User,
    frontend_url: &Url,
    dottorrent_files_path: &PathBuf,
) -> Result<Torrent> {
    let create_torrent_query = r#"
    INSERT INTO torrents (
        edition_group_id, created_by_id, release_name, 
        release_group, description, file_amount_per_type, uploaded_as_anonymous, 
        file_list, mediainfo, trumpable, staff_checked, size,
        duration, audio_codec, audio_bitrate, audio_bitrate_sampling,
        audio_channels, video_codec, features, subtitle_languages, video_resolution, container,
        language
    ) VALUES (
        $1, $2, $3, 
        $4, $5, $6, $7, 
        $8, $9, $10, $11, $12,
        $13, $14::audio_codec_enum, $15, $16::audio_bitrate_sampling_enum,
        $17, $18::video_codec_enum, $19::features_enum[], $20, $21, $22, $23
    ) RETURNING *;
    "#;

    let metainfo =
        Metainfo::from_bytes::<Vec<u8>>(torrent_form.torrent_file.data.clone().into()).unwrap();
    // let file_list = metainfo
    //     .info()
    //     .files()
    //     .map(|file| {
    //         let dir = metainfo.info().directory();
    //         let file_path = file.path().to_str().unwrap();
    //         if !dir.is_none() {
    //             format!("{}/{}", dir.unwrap().to_str().unwrap(), file_path)
    //         } else {
    //             file_path.to_string()
    //         }
    //     })
    //     .collect::<Vec<String>>();

    let info = metainfo.info();
    // TODO: torrent metadata extraction should be done on the client side
    let parent_folder = info.directory().map(|d| d.to_str().unwrap()).unwrap_or("");
    let files = info
        .files()
        .map(|f| json!({"name": f.path().to_str().unwrap(), "size": f.length()}))
        .collect::<Vec<_>>();

    let file_list = json!({"parent_folder": parent_folder, "files": files});

    let file_amount_per_type = json!(
        metainfo
            .info()
            .files()
            .flat_map(|file| file.path().to_str().unwrap().split('.').last())
            .fold(std::collections::HashMap::new(), |mut acc, ext| {
                *acc.entry(ext.to_string()).or_insert(0) += 1;
                acc
            })
    );

    // TODO: check if the torrent is trumpable: via a service ?
    let trumpable = String::from("");
    let size = metainfo
        .info()
        .files()
        .map(|file| file.length())
        .sum::<u64>() as i64;

    let uploaded_torrent = sqlx::query_as::<_, Torrent>(create_torrent_query)
        .bind(&torrent_form.edition_group_id.0)
        .bind(&current_user.id)
        .bind(&*torrent_form.release_name.0)
        .bind(&*torrent_form.release_group.0)
        .bind(torrent_form.description.as_deref())
        .bind(&file_amount_per_type)
        .bind(&torrent_form.uploaded_as_anonymous.0)
        .bind(&file_list)
        .bind(&*torrent_form.mediainfo.0)
        .bind(&trumpable)
        .bind(&false)
        .bind(&size)
        .bind(torrent_form.duration.as_deref())
        .bind(torrent_form.audio_codec.as_deref())
        .bind(torrent_form.audio_bitrate.as_deref())
        .bind(torrent_form.audio_bitrate_sampling.as_deref())
        .bind(torrent_form.audio_channels.as_deref())
        .bind(torrent_form.video_codec.as_deref())
        .bind(if torrent_form.features.is_empty() {
            Vec::new()
        } else {
            torrent_form
                .features
                .split(',')
                .map(|f| Features::from_str(f).ok().unwrap())
                .collect::<Vec<Features>>()
        })
        .bind(if torrent_form.subtitle_languages.is_empty() {
            Vec::new()
        } else {
            torrent_form
                .subtitle_languages
                .0
                .split(',')
                .map(|f| f.trim())
                .collect::<Vec<&str>>()
        })
        .bind(torrent_form.video_resolution.as_deref())
        .bind(&*torrent_form.container)
        .bind(torrent_form.language.as_deref())
        .fetch_one(pool)
        .await
        .map_err(Error::CouldNotCreateTorrent)?;

    let title_group_info = sqlx::query_as!(
        TitleGroupInfoLite,
        r#"
            SELECT title_groups.id, title_groups.name
            FROM edition_groups
            JOIN title_groups ON edition_groups.title_group_id = title_groups.id
            WHERE edition_groups.id = $1
        "#,
        torrent_form.edition_group_id.0
    )
    .fetch_one(pool)
    .await?;

    //TODO: edit torrent file : remove announce url, add comment with torrent url, etc.
    let output_path = format!(
        "{}",
        dottorrent_files_path
            .join(format!("{}{}", uploaded_torrent.id, ".torrent"))
            .to_str()
            .unwrap()
    );
    fs::write(&output_path, &torrent_form.torrent_file.data)
        .map_err(|error| Error::CouldNotSaveTorrentFile(output_path, error.to_string()))?;

    let _ = notify_users(
        pool,
        &"torrent_uploaded",
        &title_group_info.id,
        &"New torrent uploaded subscribed title group",
        &format!(
            "New torrent uploaded in title group \"{}\"",
            title_group_info.name
        ),
    )
    .await;

    Ok(uploaded_torrent)
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct TorrentSearchResults {
    pub title_groups: Vec<TitleGroupHierarchyLite>,
}

pub async fn search_torrents(pool: &PgPool, torrent_search: &TorrentSearch) -> Result<Value> {
    let search_results = sqlx::query!(
        r#"
           WITH title_group_search AS (
    SELECT
        id AS title_group_id,
        CASE
            WHEN $1 = '' THEN NULL
            ELSE ts_rank_cd(to_tsvector('simple', name || ' ' || coalesce(array_to_string(name_aliases, ' '), '')), plainto_tsquery('simple', $1))
        END AS relevance
    FROM title_groups
    WHERE $1 = '' OR to_tsvector('simple', name || ' ' || coalesce(array_to_string(name_aliases, ' '), '')) @@ plainto_tsquery('simple', $1)
),
title_group_data AS (
    SELECT
        jsonb_build_object(
            'id', tg.id,
            'name', tg.name,
            'covers', tg.covers,
            'category', tg.category,
            'content_type', tg.content_type,
            'tags', tg.tags,
            'original_release_date', tg.original_release_date,
            'affiliated_artists', COALESCE((
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ar.id,
                        'name', ar.name
                    )
                )
                FROM affiliated_artists aa
                JOIN artists ar ON aa.artist_id = ar.id
                WHERE aa.title_group_id = tg.id
            ), '[]'::jsonb),
            'edition_groups', (
                SELECT COALESCE(jsonb_agg(
                    jsonb_build_object(
                        'id', eg.id,
                        'title_group_id', eg.title_group_id,
                        'name', eg.name,
                        'release_date', eg.release_date,
                        'distributor', eg.distributor,
                        'covers', eg.covers,
                        'source', eg.source,
                        'additional_information', eg.additional_information,
                        'torrents', (
                            SELECT COALESCE(jsonb_agg(
                                jsonb_build_object(
                                    'id', t.id,
                                    'edition_group_id', t.edition_group_id,
                                    'created_at', t.created_at,
                                    'release_name', t.release_name,
                                    'file_amount_per_type', t.file_amount_per_type,
                                    'trumpable', t.trumpable,
                                    'staff_checked', t.staff_checked,
                                    'language', t.language,
                                    'container', t.container,
                                    'size', t.size,
                                    'duration', t.duration,
                                    'audio_codec', t.audio_codec,
                                    'audio_bitrate', t.audio_bitrate,
                                    'audio_bitrate_sampling', t.audio_bitrate_sampling,
                                    'audio_channels', t.audio_channels,
                                    'video_codec', t.video_codec,
                                    'features', t.features,
                                    'subtitle_languages', t.subtitle_languages,
                                    'video_resolution', t.video_resolution
                                )
                            ), '[]'::jsonb)
                            FROM torrents_and_reports t
                            WHERE t.edition_group_id = eg.id
                        )
                    )
                ), '[]'::jsonb)
                FROM edition_groups eg
                WHERE eg.title_group_id = tg.id
            )
        ) AS lite_title_group,
        CASE
            WHEN $1 = '' THEN EXTRACT(EPOCH FROM tg.created_at)
            ELSE tgs.relevance
        END AS sort_order
    FROM title_groups tg
    JOIN title_group_search tgs ON tg.id = tgs.title_group_id
)
SELECT jsonb_agg(lite_title_group ORDER BY sort_order DESC) AS title_groups
FROM title_group_data;
        "#,
        torrent_search.title_group_name
    )
    .fetch_one(pool)
    .await
    .map_err(|error| Error::ErrorSearchingForTorrents(error.to_string()))?;

    Ok(serde_json::json!({"title_groups": search_results.title_groups}))
}
