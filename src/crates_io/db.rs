use super::{models::CrateInfo, Crates, Meta, Owner, Owners, SearchResult};
pub use crate::database::{
    schema::{accounts::dsl::*, crates::dsl::*},
    Paginate,
};
use crate::{auth::AccountWithId, database::Database};
use anyhow::{bail, Context, Result};
use chrono::Local;
use diesel::{associations::HasTable, dsl::exists, prelude::*};
use log::info;
use semver::Version;
use std::future::Future;

pub async fn search<Fut>(
    db: &Database,
    key_word: impl AsRef<str>,
    per_page: i64,
    page: i64,
    backup_proc: Fut,
) -> Result<SearchResult>
where
    Fut: Future<Output = Result<SearchResult>>,
{
    let key_word = key_word.as_ref();
    let filter = name
        .like(format!("%{}%", key_word))
        .or(description.like(format!("%{}%", key_word)));
    let (mut records, count) = crates
        .filter(filter)
        .paginate(page)
        .per_page(per_page)
        .load_and_count::<Crates>(&db.connection)
        .context("database search error")?;

    let result: SearchResult = if count == 0 || !records.iter().any(|r| r.name == key_word) {
        // no exact match in database, search from backup process (upstream crates.io)
        let search_result = backup_proc.await?;

        // insert records into database
        diesel::replace_into(crates::table())
            .values(&search_result.crates)
            .execute(&db.connection)
            .context("insert crates-io result to db failed")?;

        search_result
    } else {
        info!("search from cache, get result {}", count);
        records.sort_by(|a, b| a.name.len().cmp(&b.name.len()));
        SearchResult {
            crates: records,
            meta: Meta {
                total: count,
                ..Default::default()
            },
        }
    };

    Ok(result)
}

pub(super) async fn update(db: &Database, meta: CrateInfo, user: impl Into<String>) -> Result<()> {
    let new_crate;
    let record = crates
        .filter(name.eq(&meta.name))
        .first::<Crates>(&db.connection)
        .optional()?;
    if let Some(record) = record {
        new_crate = Crates {
            name: meta.name.clone(),
            id: meta.name,
            updated_at: Local::now().to_string(),
            keywords: Some(meta.keywords.join(",")),
            categories: Some(meta.categories.join(",")),
            max_version: meta.vers.clone(), // new version always the max version, we check it before
            newest_version: meta.vers.clone(),
            max_stable_version: {
                let ver = Version::parse(&meta.vers)?;
                if ver.pre.len() == 0 && ver.build.len() == 0 {
                    // it's stable
                    Some(meta.vers)
                } else {
                    record.max_stable_version
                }
            },
            description: meta.description,
            homepage: meta.homepage,
            documentation: meta.documentation,
            repository: meta.repository,
            ..record
        }
    } else {
        new_crate = Crates {
            id: meta.name.clone(),
            name: meta.name,
            updated_at: Local::now().to_string(),
            versions: None,
            keywords: Some(meta.keywords.join(",")),
            categories: Some(meta.categories.join(",")),
            created_at: Local::now().to_string(),
            downloads: 0,
            recent_downloads: 0,
            max_version: meta.vers.clone(),
            newest_version: meta.vers.clone(),
            max_stable_version: {
                let ver = Version::parse(&meta.vers)?;
                if ver.pre.len() == 0 && ver.build.len() == 0 {
                    Some(meta.vers)
                } else {
                    None
                }
            },
            description: meta.description,
            homepage: meta.homepage,
            documentation: meta.documentation,
            repository: meta.repository,
            owners: Some(user.into()),
        };
    }

    diesel::replace_into(crates::table())
        .values(new_crate)
        .execute(&db.connection)?;
    Ok(())
}

pub(super) fn get_crate(db: &Database, crate_name: impl AsRef<str>) -> Result<Crates> {
    Ok(crates::table()
        .filter(name.eq(crate_name.as_ref()))
        .first(&db.connection)?)
}

pub(super) fn get_owners(db: &Database, owner_list: &Vec<String>) -> Result<Owners> {
    let records = accounts::table()
        .filter(username.eq_any(owner_list))
        .load::<AccountWithId>(&db.connection)?;

    let results = records
        .into_iter()
        .map(|o| Owner {
            id: o.id as u32,
            login: o.username,
            name: o.display_name,
        })
        .collect();
    Ok(Owners { users: results })
}

pub(super) fn add_owner(
    db: &Database,
    crate_name: impl AsRef<str>,
    mut old_owners: Vec<String>,
    new_owners: Vec<String>,
) -> Result<()> {
    for p in &new_owners {
        if !diesel::select(exists(accounts.filter(username.eq(p))))
            .get_result::<bool>(&db.connection)?
        {
            bail!(
                "user {} not exists, can not be a owner of {}",
                p,
                crate_name.as_ref()
            );
        }
    }

    for o in &old_owners {
        if new_owners.contains(o) {
            bail!("{} already in the owner list of {}", o, crate_name.as_ref());
        }
    }

    old_owners.extend(new_owners);
    diesel::update(crates::table())
        .filter(name.eq(crate_name.as_ref()))
        .set(owners.eq(old_owners.join(",")))
        .execute(&db.connection)?;
    Ok(())
}

pub(super) fn remove_owner(
    db: &Database,
    crate_name: impl AsRef<str>,
    mut old_owners: Vec<String>,
    remove_owners: Vec<String>,
) -> Result<()> {
    for r in &remove_owners {
        if !old_owners.contains(r) {
            bail!("{} not int the owner list of {}", r, crate_name.as_ref());
        }
    }

    old_owners.retain(|o| !remove_owners.contains(o));
    diesel::update(crates::table())
        .filter(name.eq(crate_name.as_ref()))
        .set(owners.eq(old_owners.join(",")))
        .execute(&db.connection)?;
    Ok(())
}
