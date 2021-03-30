mod db;
mod index;
mod models;

use self::models::Crates;
use crate::{
    auth::{get_user_by_token, Account},
    database::Database,
    Server,
};
use anyhow::{anyhow, Context};
use futures::StreamExt;
pub use index::Index;
use log::{debug, info, warn};
use models::CrateInfo;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Digest;
use spa_server::{
    error_to_json,
    re_export::{
        delete,
        error::{ErrorBadRequest, ErrorForbidden, ErrorInternalServerError, ErrorUnauthorized},
        get, put, web, HttpRequest, HttpResponse, NamedFile, Responder, Result,
    },
};
use std::{
    fs,
    io::{BufReader, Read},
    path::Path,
    sync::Arc,
};

#[get("/{name}/{version}/download")]
pub async fn download(
    info: web::Path<(String, String)>,
    data: web::Data<Server>,
) -> Result<impl Responder> {
    let (name, version) = info.into_inner();
    let crate_name = format!("{}-{}.crate", name, version);
    let data = data.into_inner();
    let config = data.config.read().await;
    let crate_path = config.crates.storage_path.join(&name).join(&crate_name);
    let crate_file = match NamedFile::open(&crate_path) {
        Ok(f) => f,
        Err(_) => {
            warn!("{} is not in our storage, get it from upstream", crate_name);
            download_from_upstream(&crate_path, name, version, &data)
                .await
                .map_err(|e| {
                    ErrorInternalServerError(format!(
                        "download crate from upstream failed: {:?}",
                        e
                    ))
                })?;

            NamedFile::open(crate_path)?
        }
    };

    Ok(crate_file)
}

fn unsecure_http_client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        // when using mirror registry in your company, there may have a firewall use
        // self-signed certificate to sniff all the network traffic, so we ignore it here
        .danger_accept_invalid_certs(true)
        .build()?)
}

async fn download_from_upstream(
    path: impl AsRef<Path>,
    name: impl AsRef<str>,
    version: impl AsRef<str>,
    data: &Arc<Server>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let client = unsecure_http_client()?;
    let name = name.as_ref();
    let version = version.as_ref();
    for i in 0u32..5 {
        let response = client
            .get(
                Url::parse(&format!(
                    "{}/api/v1/crates/{}/{}/download",
                    data.config.read().await.crates.upstream_url,
                    name,
                    version
                ))
                .unwrap(),
            )
            .send()
            .await?;

        let bytes = response.bytes().await?;
        let checksum = format!("{:x}", sha2::Sha256::digest(&bytes));
        let meta = data.index.get_exact(name, version).await?;
        if meta.cksum != checksum {
            warn!("checksum not match, try download again, retry time: {}", i);
            continue;
        }

        let dir = path.parent().ok_or(anyhow!("no parent for {:?}", path))?;
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, bytes)?;
        break;
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct SearchParam {
    q: String,
    per_page: i64,
    page: Option<i64>,
}

#[allow(dead_code)]
#[derive(Serialize, Default, Deserialize)]
struct Meta {
    total: i64,
    #[serde(skip_serializing)]
    next_page: Option<String>,
    #[serde(skip_serializing)]
    prev_page: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct SearchResult {
    crates: Vec<Crates>,
    meta: Meta,
}

#[derive(Serialize, Debug)]
pub struct Owner {
    id: u32,
    login: String,
    name: String,
}

#[derive(Serialize)]
pub struct Owners {
    users: Vec<Owner>,
}

#[error_to_json]
#[get("")]
pub async fn search(
    param: web::Query<SearchParam>,
    data: web::Data<Server>,
) -> Result<HttpResponse> {
    let page = param.page.unwrap_or(1);
    let key_word = param.q.clone();
    let upstream_url = data.config.read().await.crates.upstream_url.clone();
    let result = db::search(
        &*data.database.lock().await,
        key_word.clone(),
        param.per_page,
        page,
        async move {
            info!("no exact match, search from crates-io instead");
            let client = unsecure_http_client()?;
            let resp = client
                .get(
                    Url::parse(&format!(
                        "{}/api/v1/crates?q={}&per_page={}&page={}",
                        upstream_url, key_word, &param.per_page, page,
                    ))
                    .unwrap(),
                )
                .header("User-Agent", "mirror_registry (avalon1610@gmail.com)")
                .send()
                .await
                .context("search from crates-io failed")?;
            if !resp.status().is_success() {
                return Err(anyhow!(
                    "crates-io return {}, reason: {}, ",
                    resp.status(),
                    resp.text().await.unwrap_or("unknown".to_string())
                ));
            }

            let search_result: SearchResult = resp
                .json()
                .await
                .context("convert crates-io result failed")?;

            Ok(search_result)
        },
    )
    .await
    .map_err(|e| ErrorInternalServerError(format!("search internal error: {:?}", e)))?;

    Ok(HttpResponse::Ok().json(&result))
}

fn check_token(db: &Database, req: HttpRequest) -> anyhow::Result<Account> {
    let token = req
        .headers()
        .get("Authorization")
        .ok_or(anyhow!("need token for authorization"))?;

    get_user_by_token(db, token.to_str()?)
}

fn check_owner(
    db: &Database,
    req: HttpRequest,
    crate_name: impl AsRef<str>,
) -> anyhow::Result<(String, Vec<String>)> {
    let account = check_token(&db, req)?;
    let crate_info = db::get_crate(&db, &crate_name).context("get crate failed")?;
    check_owner_impl(&account, crate_info.owners, &crate_name)
}

fn check_owner_impl(
    account: &Account,
    owners: Option<String>,
    crate_name: impl AsRef<str>,
) -> anyhow::Result<(String, Vec<String>)> {
    let owners = owners.ok_or(anyhow!(
        "no owner found, this is an upstream crate, can not be modified"
    ))?;

    let owners: Vec<&str> = owners.split(",").collect();
    for owner in owners.iter() {
        if account.username == *owner {
            return Ok((
                account.username.clone(),
                owners.into_iter().map(|s| s.to_string()).collect(),
            ));
        }
    }

    Err(anyhow!(
        "{} not in the owners of {}",
        account.username,
        crate_name.as_ref()
    ))
}

#[derive(Serialize)]
struct QuickOk {
    ok: bool,
}

fn quick_ok() -> QuickOk {
    QuickOk { ok: true }
}

#[error_to_json]
#[put("new")]
async fn publish(
    req: HttpRequest,
    mut body: web::Payload,
    data: web::Data<Server>,
) -> Result<HttpResponse> {
    let db = data.database.lock().await;
    let account = check_token(&db, req).map_err(|e| ErrorUnauthorized(e))?;

    let mut bytes = web::BytesMut::new();
    while let Some(b) = body.next().await {
        bytes.extend_from_slice(&b?);
    }

    let (crate_info, crate_data) = create_crate(&*bytes).map_err(|e| ErrorBadRequest(e))?;
    if let Ok(old_crate) = db::get_crate(&db, &crate_info.name) {
        check_owner_impl(&account, old_crate.owners, &crate_info.name)
            .map_err(|e| ErrorForbidden(e))?;
    }

    let cksum = format!("{:x}", sha2::Sha256::digest(&crate_data));

    // update database
    db::update(&db, crate_info.clone(), &account.username)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    // store the crate data
    let crate_name = format!("{}-{}.crate", &crate_info.name, &crate_info.vers);
    let crate_path = data
        .config
        .read()
        .await
        .crates
        .storage_path
        .join(&crate_info.name)
        .join(&crate_name);
    let dir = crate_path
        .parent()
        .ok_or(anyhow!("no parent for {:?}", crate_path))
        .map_err(|e| ErrorInternalServerError(e))?;
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    fs::write(crate_path, crate_data)?;

    // update work tree, and sync
    let commit_msg = format!("add crate {}-{}", &crate_info.name, &crate_info.vers);
    data.index
        .append(crate_info, cksum)
        .await
        .map_err(|e| ErrorBadRequest(e))?;
    data.git
        .commit(commit_msg)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    data.git
        .sync_index()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    info!("{} published new crate {}", account.username, &crate_name);
    Ok(HttpResponse::Ok().json(quick_ok()))
}

fn create_crate(bytes: &[u8]) -> anyhow::Result<(CrateInfo, Vec<u8>)> {
    let mut reader = BufReader::new(bytes);

    let mut metadata_len_bytes = [0u8; 4];
    reader.read_exact(&mut metadata_len_bytes)?;
    let metadata_len = u32::from_le_bytes(metadata_len_bytes);

    let mut metadata = vec![0u8; metadata_len as usize];
    reader.read_exact(&mut metadata)?;
    let metadata_json = String::from_utf8_lossy(&metadata);

    let mut crate_len_bytes = [0u8; 4];
    reader.read_exact(&mut crate_len_bytes)?;
    let crate_len = u32::from_le_bytes(crate_len_bytes);

    let mut crate_data = vec![0u8; crate_len as usize];
    reader.read_exact(&mut crate_data)?;

    debug!(
        "new crate publish:\n{}\ncrate len:{}",
        metadata_json, crate_len
    );

    Ok((serde_json::from_str(&metadata_json)?, crate_data))
}

#[error_to_json]
#[delete("{crate_name}/{version}/yank")]
pub async fn yank(
    req: HttpRequest,
    data: web::Data<Server>,
    info: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (name, version) = info.into_inner();
    let db = data.database.lock().await;
    let (username, _) = check_owner(&db, req, &name).map_err(|e| ErrorForbidden(e))?;

    let index = &data.index;
    index
        .set_yank(&name, &version, true)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    data.git
        .commit(format!("yank {}-{}", name, version))
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    data.git
        .sync_index()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    info!("{} yanked crate {}-{}", username, name, version);
    Ok(HttpResponse::Ok().json(quick_ok()))
}

#[error_to_json]
#[put("{crate_name}/{version}/unyank")]
pub async fn unyank(
    req: HttpRequest,
    data: web::Data<Server>,
    info: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (name, version) = info.into_inner();
    let db = data.database.lock().await;
    let (username, _) = check_owner(&db, req, &name).map_err(|e| ErrorForbidden(e))?;

    let index = &data.index;
    index
        .set_yank(&name, &version, false)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    data.git
        .commit(format!("unyank {}-{}", name, version))
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    data.git
        .sync_index()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    info!("{} unyanked crate {}-{}", username, name, version);
    Ok(HttpResponse::Ok().json(quick_ok()))
}

#[error_to_json]
#[get("{crate_name}/owners")]
pub async fn list_owners(
    req: HttpRequest,
    data: web::Data<Server>,
    info: web::Path<(String,)>,
) -> Result<HttpResponse> {
    let (crate_name,) = info.into_inner();
    let db = data.database.lock().await;
    let (_, owners) = check_owner(&db, req, &crate_name).map_err(|e| ErrorForbidden(e))?;
    let owners = db::get_owners(&db, &owners).map_err(|e| ErrorInternalServerError(e))?;

    info!("list owners for {}: {:?}", crate_name, owners.users);
    Ok(HttpResponse::Ok().json(owners))
}

#[derive(Deserialize)]
pub struct Users {
    users: Vec<String>,
}

#[error_to_json]
#[put("{crate_name}/owners")]
pub async fn add_owner(
    req: HttpRequest,
    data: web::Data<Server>,
    path_info: web::Path<(String,)>,
    json_info: web::Json<Users>,
) -> Result<HttpResponse> {
    let (crate_name,) = path_info.into_inner();
    let db = data.database.lock().await;
    let (_, old_owners) = check_owner(&db, req, &crate_name).map_err(|e| ErrorForbidden(e))?;
    let new_users = json_info.into_inner().users;
    let msg = format!(
        "user {:?} has been added to be an owner of crate {}",
        &new_users, &crate_name
    );
    db::add_owner(&db, crate_name, old_owners, new_users)
        .map_err(|e| ErrorInternalServerError(e))?;

    let result = json!({"ok": true, "msg": msg});
    Ok(HttpResponse::Ok().body(result.to_string()))
}

#[error_to_json]
#[delete("{crate_name}/owners")]
pub async fn remove_owner(
    req: HttpRequest,
    data: web::Data<Server>,
    path_info: web::Path<(String,)>,
    json_info: web::Json<Users>,
) -> Result<HttpResponse> {
    let (crate_name,) = path_info.into_inner();
    let db = data.database.lock().await;
    let (_, old_owners) = check_owner(&db, req, &crate_name).map_err(|e| ErrorForbidden(e))?;

    if old_owners.len() == 1 {
        return Err(ErrorBadRequest(format!(
            "crate {} has only one owner, can not remove anymore",
            &crate_name
        )));
    }

    let remove_owners = json_info.into_inner().users;
    let msg = format!(
        "user {:?} has been removed from the owners of crate {}",
        &remove_owners, &crate_name
    );
    db::remove_owner(&db, crate_name, old_owners, remove_owners)
        .map_err(|e| ErrorInternalServerError(e))?;

    let result = json!({"ok": true, "msg": msg});
    Ok(HttpResponse::Ok().body(result.to_string()))
}
