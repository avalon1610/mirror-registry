use super::{
    account::{AccountRole, AccountType},
    check, get_user_by_name, rand_str, Account, UserContext,
};
use crate::{config, Server};
use anyhow::anyhow;
use anyhow::bail;
use chrono::Local;
use ldap3::{LdapConnAsync, Scope, SearchEntry};
use log::{info, warn};
use spa_server::re_export::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    get,
    web::{self, Query},
    HttpRequest, HttpResponse, Identity, Responder, Result,
};
use std::cell::RefCell;
use std::collections::HashMap;
use tokio::sync::Mutex;

pub(super) struct Ldap {
    inner: Option<RefCell<ldap3::Ldap>>,
    config: config::Ldap,
    cache: Mutex<HashMap<String, LdapAccount>>,
}

struct LdapAccount {
    username: String,
    display_name: String,
    email: String,
}

impl Ldap {
    pub fn new() -> Self {
        Ldap {
            inner: None,
            config: config::Ldap::default(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect(&mut self, cfg: config::Ldap) -> anyhow::Result<()> {
        let (conn, inner) = LdapConnAsync::new(&format!("ldap://{}", &cfg.hostname)).await?;
        ldap3::drive!(conn);
        self.inner = Some(RefCell::new(inner));
        self.config = cfg;

        Ok(())
    }

    pub async fn search_user(&self, username: impl AsRef<str>) -> anyhow::Result<Option<Account>> {
        let username = username.as_ref();
        let mut cache = self.cache.lock().await;
        let ldap_account = match cache.get(username) {
            Some(a) => a,
            None => {
                self.login(&self.config.username, &self.config.password)
                    .await?;

                let (result, _) = self
                    .inner
                    .as_ref()
                    .ok_or(anyhow!("ldap server not connected"))?
                    .borrow_mut()
                    .search(
                        &self.config.base_dn,
                        Scope::Subtree,
                        "(objectclass=person)",
                        vec!["sAMAccountName", "cn", "mail"],
                    )
                    .await?
                    .success()?;

                let default = vec!["unknown".to_string()];
                for r in result.into_iter() {
                    let attrs = SearchEntry::construct(r).attrs;
                    let cn = &attrs.get("cn").unwrap_or(&default)[0];
                    let sam = &attrs.get("sAMAccountName").unwrap_or(&default)[0];
                    let mail = &attrs.get("mail").unwrap_or(&default)[0];

                    let id = sam.clone();
                    cache.insert(
                        id,
                        LdapAccount {
                            username: sam.clone(),
                            display_name: cn.clone(),
                            email: mail.clone(),
                        },
                    );
                }

                match cache.get(username) {
                    Some(a) => a,
                    None => return Ok(None),
                }
            }
        };

        let mut account = Account::new(
            &ldap_account.username,
            AccountType::Ldap.as_ref(),
            AccountRole::User.as_ref(),
        );

        account
            .display_name(&ldap_account.display_name)
            .email(&ldap_account.email);
        Ok(Some(account))
    }

    async fn login(
        &self,
        username: impl AsRef<str>,
        password: impl AsRef<str>,
    ) -> anyhow::Result<()> {
        let r = self
            .inner
            .as_ref()
            .ok_or(anyhow!("ldap server not connected"))?
            .borrow_mut()
            .simple_bind(
                &format!("{}@{}", username.as_ref(), &self.config.domain),
                password.as_ref(),
            )
            .await?
            .success()?;
        if r.rc != 0 {
            bail!("error from ldap server: {}", r.text);
        }

        Ok(())
    }
}

fn unauthorized(msg: impl Into<String>) -> Result<HttpResponse> {
    Ok(HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", "Basic"))
        .body(msg.into()))
}

const BASIC_MASK: &str = "Basic";
fn parse_auth(auth: impl AsRef<str>) -> anyhow::Result<(String, String)> {
    let auth = auth.as_ref();
    let (mark, content) = auth.split_at(BASIC_MASK.len());
    let content = content.trim();
    if mark != BASIC_MASK {
        bail!("only support basic authorization");
    }

    let bytes = base64::decode(content.as_bytes())?;
    let auth_str = String::from_utf8_lossy(&bytes);
    let sp: Vec<&str> = auth_str.split(':').collect();
    if sp.len() != 2 {
        bail!("invalid authorization");
    }

    Ok((sp[0].to_string(), sp[1].to_string()))
}

#[get("/ldap_login")]
pub async fn login(
    req: HttpRequest,
    data: web::Data<Server>,
    id: Identity,
) -> Result<impl Responder> {
    let db = data.database.lock().await;
    if let Ok(user) = check(&id, &db) {
        if let Some(token) = user.token {
            return Ok(HttpResponse::Ok().json(UserContext {
                username: user.username,
                role: user.role,
                token,
                r#type: user.type_,
            }));
        }
    }

    let ldap = match &data.config.read().await.registry.ldap {
        Some(ldap_cfg) => {
            let mut ldap = data.auth_context.ldap.lock().await;
            ldap.connect(ldap_cfg.clone())
                .await
                .map_err(|e| ErrorInternalServerError(e))?;
            ldap
        }

        None => return Err(ErrorBadRequest("ldap not enabled")),
    };

    if let Some(auth) = req.headers().get("Authorization") {
        let (username, password) = match parse_auth(auth.to_str().map_err(|e| ErrorBadRequest(e))?)
        {
            Ok(r) => r,
            Err(e) => return unauthorized(format!("{:?}", e)),
        };

        let mut user =
            match get_user_by_name(&db, &username).map_err(|e| ErrorInternalServerError(e))? {
                Some(u) => {
                    if u.type_ != AccountType::Ldap.as_ref() {
                        return unauthorized("invalid login type");
                    }
                    u
                }
                None => {
                    if let Some(u) = ldap
                        .search_user(&username)
                        .await
                        .map_err(|e| ErrorInternalServerError(e))?
                    {
                        u.insert(&db).map_err(|e| ErrorInternalServerError(e))?;
                        u
                    } else {
                        return unauthorized("invalid username or password");
                    }
                }
            };

        if let Err(e) = ldap.login(&username, password).await {
            warn!("{:?}", e);
            return unauthorized("invalid username or password");
        }

        info!(
            "remote: {} user: {} login ok via LDAP",
            req.connection_info().remote_addr().unwrap_or("<unknown>"),
            &username
        );

        user.last_login(Local::now().to_string())
            .token(rand_str(64))
            .update(&db)
            .map_err(|e| ErrorInternalServerError(e))?;
        id.remember(username.clone());

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

    unauthorized("cancelled")
}
