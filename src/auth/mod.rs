mod account;
mod ldap;
mod models;

pub use self::ldap::login as ldap_login;
pub use self::models::{Account, AccountWithId};
use self::{account::AccountRole, ldap::Ldap};
use crate::{database::Database, Server};
use account::AccountType;
pub use account::{get_user_by_name, get_user_by_token, setup_root, SALT};
use anyhow::{anyhow, bail};
use chrono::Local;
use error::{ErrorBadRequest, ErrorInternalServerError, ErrorUnauthorized};
use log::{debug, info, warn};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use spa_server::re_export::{
    error::{self, ErrorForbidden},
    get, post,
    web::{self, Query},
    HttpRequest, HttpResponse, Identity, Responder, Result,
};
use std::collections::{HashMap, VecDeque};
use tokio::sync::Mutex;

pub(crate) struct AuthContext {
    nonce_list: Mutex<VecDeque<(String, String)>>,
    ldap: Mutex<Ldap>,
}

impl AuthContext {
    pub async fn new() -> anyhow::Result<AuthContext> {
        Ok(AuthContext {
            nonce_list: Mutex::new(VecDeque::new()),
            ldap: Mutex::new(Ldap::new()),
        })
    }
}

#[derive(Default, Debug)]
struct Authorization {
    username: String,
    realm: String,
    nonce: String,
    uri: String,
    qop: String,
    nc: String,
    cnonce: String,
    response: String,
    opaque: String,
}

#[derive(Serialize)]
struct UserContext {
    username: String,
    token: String,
    role: String,
    r#type: String,
}

const DIGEST_MARK: &str = "Digest";
fn parse_auth<S: AsRef<str>>(auth: S) -> anyhow::Result<Authorization> {
    let auth = auth.as_ref();
    let (mark, content) = auth.split_at(DIGEST_MARK.len());
    let content = content.trim();
    if mark != DIGEST_MARK {
        bail!("only support digest authorization");
    }

    let mut result = Authorization::default();
    for c in content.split(",").into_iter() {
        let c = c.trim();
        let i = c
            .find('=')
            .ok_or(anyhow!("invalid part of authorization: {}", c))?;

        let (k, v) = c.split_at(i);
        let v = v.trim_start_matches('=').trim_matches('"');
        match k {
            "username" => result.username = v.to_string(),
            "realm" => result.realm = v.to_string(),
            "nonce" => result.nonce = v.to_string(),
            "uri" => result.uri = v.to_string(),
            "qop" => result.qop = v.to_string(),
            "nc" => result.nc = v.to_string(),
            "cnonce" => result.cnonce = v.to_string(),
            "response" => result.response = v.to_string(),
            "opaque" => result.opaque = v.to_string(),
            _ => {
                warn!("unknown authorization part: {}", c);
                continue;
            }
        }
    }

    Ok(result)
}

async fn unauthorized(data: &web::Data<Server>, msg: impl Into<String>) -> Result<HttpResponse> {
    let nonce = rand_str(32);
    let opaque = rand_str(32);

    let www_authenticate = format!(
        r#"Digest realm="{}",qop="auth",nonce="{}",opaque="{}""#,
        &SALT.lock().await,
        nonce,
        opaque
    );

    {
        let mut nonce_list = data.auth_context.nonce_list.lock().await;
        while nonce_list.len() >= 256 {
            nonce_list.pop_front();
        }

        nonce_list.push_back((nonce, opaque));
    }

    Ok(HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", www_authenticate))
        .body(msg.into()))
}

#[get("/login")]
pub async fn login(
    req: HttpRequest,
    id: Identity,
    data: web::Data<Server>,
) -> Result<impl Responder> {
    let conn = &*data.database.lock().await;
    if let Ok(user) = check(&id, conn) {
        if let Some(token) = user.token {
            return Ok(HttpResponse::Ok().json(UserContext {
                username: user.username,
                role: user.role,
                r#type: user.type_,
                token,
            }));
        }
    }

    if let Some(auth) = req.headers().get("Authorization") {
        let auth = match parse_auth(auth.to_str().map_err(|e| ErrorBadRequest(e))?) {
            Ok(a) => a,
            Err(e) => return unauthorized(&data, format!("{:?}", e)).await,
        };

        debug!("get auth: {:?}", auth);
        if !auth.uri.starts_with("/auth/login") {
            return unauthorized(&data, "authorization uri not match").await;
        }

        let mut found_nonce = false;
        {
            let mut nonce_list = data.auth_context.nonce_list.lock().await;
            let mut index = nonce_list.len().saturating_sub(1);
            for (nonce, opaque) in nonce_list.iter().rev() {
                if nonce == &auth.nonce || opaque == &auth.opaque {
                    found_nonce = true;
                    nonce_list.remove(index);
                    break;
                }

                index = index.saturating_sub(1);
            }
        }

        if !found_nonce {
            return unauthorized(&data, "invalid nonce or opaque").await;
        }

        if auth.qop != "auth" {
            return unauthorized(&data, "only support qop = auth").await;
        }

        let mut user = match get_user_by_name(conn, &auth.username).map_err(|e| {
            ErrorInternalServerError(format!("get user failed from database: {:?}", e))
        })? {
            Some(u) => {
                if u.type_ != AccountType::Internal.as_ref() {
                    return unauthorized(&data, "invalid login type").await;
                }
                u
            }
            None => {
                return unauthorized(&data, "invalid username or password").await;
            }
        };
        let ha1 = &user.password;
        let ha2 = md5::compute(format!("{}:{}", req.method().to_string(), req.uri()));
        let password = md5::compute(format!(
            "{}:{}:{}:{}:{}:{:x}",
            ha1, auth.nonce, auth.nc, auth.cnonce, auth.qop, ha2
        ));

        if format!("{:x}", password) != auth.response {
            warn!(
                "remote: {} user: {} wrong username or password",
                req.connection_info().remote_addr().unwrap_or("<unknown>"),
                auth.username
            );
            return unauthorized(&data, "invalid username or password").await;
        }

        info!(
            "remote: {} user: {} login ok",
            req.connection_info().remote_addr().unwrap_or("<unknown>"),
            &auth.username
        );

        user.last_login(Local::now().to_string())
            .token(rand_str(64))
            .update(conn)
            .map_err(|e| ErrorInternalServerError(e))?;

        id.remember(auth.username.clone());
        let query_string = req.query_string();
        if !query_string.is_empty() {
            let query = Query::<HashMap<String, String>>::from_query(query_string)?;
            if let Some(redirect_url) = query.get("redirect") {
                return Ok(HttpResponse::TemporaryRedirect()
                    .append_header(("Location", &**redirect_url))
                    .finish());
            }
        }

        return Ok(HttpResponse::Ok().json(UserContext {
            username: user.display_name,
            token: user.token.unwrap(),
            role: user.role,
            r#type: user.type_,
        }));
    }

    unauthorized(&data, "cancelled").await
}

fn rand_str(num: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(num)
        .map(char::from)
        .collect()
}

#[get("/logout")]
pub(crate) async fn logout(id: Identity) -> Result<impl Responder> {
    id.forget();
    Ok(HttpResponse::Ok())
}

#[post("/modify")]
pub(crate) async fn modify(
    id: Identity,
    data: web::Data<Server>,
    info: web::Json<NewAccount>,
) -> Result<HttpResponse> {
    let db = data.database.lock().await;
    let op_account = check(&id, &db)?;
    let new_account = info.into_inner();
    let found_account = get_user_by_name(&db, &new_account.username)
        .map_err(|e| ErrorInternalServerError(e))?
        .ok_or(ErrorBadRequest(format!(
            "no such user: [{}]",
            &new_account.username
        )))?;
    if found_account.type_ == AccountType::Ldap.as_ref() {
        return Err(ErrorForbidden("can not modify LDAP user"));
    }

    if op_account.role == AccountRole::Root.as_ref() || op_account.username == new_account.username
    {
        db::update_account(&db, new_account).map_err(|e| ErrorInternalServerError(e))?;
        return Ok(HttpResponse::Ok().finish());
    }

    Err(ErrorForbidden(
        "only the root user or oneself can change the password",
    ))
}

#[get("who")]
async fn who(id: Identity, data: web::Data<Server>) -> Result<impl Responder> {
    let name = match id.identity() {
        Some(user) => user,
        None => {
            return Ok(HttpResponse::MovedPermanently()
                .append_header(("Location", "/auth/login?redirect=/auth/who"))
                .finish());
        }
    };

    let account = get_user_by_name(&*data.database.lock().await, name)
        .map_err(|e| ErrorInternalServerError(e))?;
    if let Some(account) = account {
        Ok(HttpResponse::Ok().json(UserContext {
            username: account.username,
            role: account.role,
            r#type: account.type_,
            token: match account.token {
                Some(tk) => tk,
                None => return unauthorized(&data, "session timeout").await,
            },
        }))
    } else {
        unauthorized(&data, "no such user").await
    }
}

pub(crate) fn check(id: &Identity, db: &Database) -> Result<Account> {
    if let Some(id) = id.identity() {
        return Ok(get_user_by_name(db, id)
            .map_err(|e| ErrorInternalServerError(e))?
            .ok_or(ErrorBadRequest("invalid session"))?);
    }

    Err(ErrorUnauthorized("You need login first"))
}

#[post("create")]
async fn create(
    new_account: web::Json<NewAccount>,
    data: web::Data<Server>,
) -> Result<impl Responder> {
    let new_account = new_account.into_inner();
    let cfg = data.config.read().await;
    if !cfg.registry.can_create_account {
        return Err(ErrorBadRequest("Account creation has been disabled"));
    }

    let mut account = Account::new(
        new_account.username,
        AccountType::Internal.as_ref(),
        AccountRole::User.as_ref(),
    );
    account
        .encoded_password(new_account.password)
        .salt(SALT.lock().await.clone());
    if let Some(email) = new_account.email {
        account.email(email);
    }

    let db = data.database.lock().await;
    db::create_account(&*db, &account)
        .map_err(|e| ErrorBadRequest(format!("create account failed: {:?}", e)))?;

    info!("created new account {}", account.username);
    Ok(HttpResponse::Ok())
}

mod db {
    use super::{models::Account, NewAccount};
    use crate::database::{schema::accounts::dsl::*, Database};
    use anyhow::{bail, Result};
    use diesel::{associations::HasTable, dsl::count_star, prelude::*};
    pub(super) fn create_account(db: &Database, account: &Account) -> Result<()> {
        let count = accounts
            .filter(username.eq(&account.username))
            .select(count_star())
            .first::<i64>(&db.connection)?;

        if count != 0 {
            bail!("account {} already exists", &account.username);
        }

        diesel::insert_into(accounts)
            .values(account)
            .execute(&db.connection)?;
        Ok(())
    }

    pub(super) fn update_account(db: &Database, new_account: NewAccount) -> Result<()> {
        match new_account.email.as_ref() {
            Some(e) => diesel::update(accounts::table())
                .filter(username.eq(new_account.username))
                .set((password.eq(new_account.password), email.eq(e)))
                .execute(&db.connection)?,
            None => diesel::update(accounts::table())
                .filter(username.eq(new_account.username))
                .set(password.eq(new_account.password))
                .execute(&db.connection)?,
        };

        Ok(())
    }
}

#[derive(Deserialize)]
pub(crate) struct NewAccount {
    username: String,
    password: String,
    email: Option<String>,
}
