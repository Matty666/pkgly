use ahash::HashMap;
use nr_core::database::entities::project::versions::DBProjectVersion;
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::NPMRegistryError;

pub(super) fn publish_tags(
    mut tags: HashMap<String, String>,
    version: &str,
) -> Result<HashMap<String, String>, NPMRegistryError> {
    if tags.is_empty() {
        tags.insert("latest".into(), version.into());
    }
    if tags
        .iter()
        .any(|(tag, target)| !valid_tag(tag) || target != version)
    {
        return Err(NPMRegistryError::InvalidDistTags);
    }
    Ok(tags)
}

pub(super) fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && semver::VersionReq::parse(tag.strip_prefix('v').unwrap_or(tag)).is_err()
}

pub(super) fn legacy_dist_tags<'a>(
    versions: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, String> {
    let mut tags = HashMap::default();
    if let Some(version) = versions
        .into_iter()
        .find(|version| semver::Version::parse(version).is_ok_and(|parsed| parsed.pre.is_empty()))
    {
        tags.insert("latest".into(), version.into());
    }
    tags
}

pub(super) fn parse_tag_path(path: &str) -> Option<(String, Option<String>)> {
    let mut components = path.strip_prefix("-/package/")?.split('/');
    let first = components.next()?;
    let name = if first.starts_with('@') {
        format!("{first}/{}", components.next()?)
    } else {
        first.to_owned()
    };
    if name.is_empty() || components.next()? != "dist-tags" {
        return None;
    }
    let tag = components.next().map(str::to_owned);
    if components.next().is_some() || tag.as_deref().is_some_and(|tag| !valid_tag(tag)) {
        return None;
    }
    Some((name, tag))
}

pub(super) async fn load_tags(
    project: Uuid,
    database: &PgPool,
) -> Result<HashMap<String, String>, NPMRegistryError> {
    let tags: Option<Json<HashMap<String, String>>> =
        sqlx::query_scalar("SELECT npm_dist_tags FROM projects WHERE id = $1")
            .bind(project)
            .fetch_one(database)
            .await?;
    if let Some(Json(tags)) = tags {
        return Ok(tags);
    }
    let versions = DBProjectVersion::get_all_versions(project, database).await?;
    Ok(legacy_dist_tags(
        versions.iter().map(|version| version.version.as_str()),
    ))
}

pub(super) async fn save_tags(
    project: Uuid,
    tags: HashMap<String, String>,
    fallback: HashMap<String, String>,
    database: &PgPool,
) -> Result<(), NPMRegistryError> {
    sqlx::query(
        "UPDATE projects SET npm_dist_tags = COALESCE(npm_dist_tags, $2) || $3 WHERE id = $1",
    )
    .bind(project)
    .bind(Json(fallback))
    .bind(Json(tags))
    .execute(database)
    .await?;
    Ok(())
}

pub(super) async fn remove_tag(
    project: Uuid,
    tag: &str,
    database: &PgPool,
) -> Result<(), NPMRegistryError> {
    let fallback = load_tags(project, database).await?;
    sqlx::query(
        "UPDATE projects SET npm_dist_tags = COALESCE(npm_dist_tags, $2) - $3 WHERE id = $1",
    )
    .bind(project)
    .bind(Json(fallback))
    .bind(tag)
    .execute(database)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
