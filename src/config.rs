use crate::auth::{check, SALT};
use crate::Server;
use anyhow::{anyhow, Context};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spa_server::re_export::{
    error::ErrorInternalServerError, get, post, web, HttpResponse, Identity, Responder, Result,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[allow(dead_code)]
const CRATES_IO_INDEX_URL: &str = "https://github.com/rust-lang/crates.io-index";
const CREATE_IO_URL: &str = "https://crates.io";
const CONFIG_PATH: &str = "mirror.registry.toml";
pub static DEFAULT_PORT: u16 = 55555;

#[derive(Serialize, Deserialize)]
pub struct Config {
    /// a set of configurations related to git repository
    pub git: Git,
    /// a set of configurations relates to crates storage
    pub crates: Crates,
    /// a set of configurations related to general config
    pub registry: Registry,
    /// a set of configurations related to database
    pub database: Database,
}

#[derive(Serialize, Deserialize)]
pub struct Crates {
    /// crate file store path
    pub storage_path: PathBuf,
    /// update crates url, default is https://crates.io
    pub upstream_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Registry {
    /// can disable account registation when ldap is on
    pub can_create_account: bool,
    /// mirror registry address, can be a domain name or an IP.
    pub address: String,
    /// sync interval, default is 6 hours
    pub interval: std::time::Duration,
    /// ldap config
    pub ldap: Option<Ldap>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Ldap {
    /// ldap server hostname
    pub hostname: String,
    /// ldap base dn
    pub base_dn: String,
    /// domain name
    pub domain: String,
    /// admin username for bind
    pub username: String,
    /// admin password for bind, plain text here
    pub password: String,
}

impl PartialEq for Ldap {
    fn eq(&self, other: &Self) -> bool {
        self.hostname == other.hostname
            && self.base_dn == other.base_dn
            && self.domain == other.domain
            && self.username == other.username
            && self.password == other.password
    }
}

#[derive(Serialize, Deserialize)]
pub struct Database {
    /// database url, here will be a sqlite3 database file path
    pub url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Git {
    /// index remote repo path [bare repo], default is ./index.git
    pub index_path: PathBuf,
    /// working index repo path, default is ./index.work.git
    pub working_path: PathBuf,
    /// upstream index uri, default is https://github.com/rust-lang/crates.io-index
    pub upstream_url: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut local_ip = None;
        for iface in pnet::datalink::interfaces() {
            if !iface.is_loopback() && iface.is_up() {
                local_ip = Some(format!("{}:{}", iface.ips[0].ip(), DEFAULT_PORT));
            }
        }

        if local_ip.is_none() {
            panic!("can not found local ip");
        }

        let home_env = env::var("HOME").unwrap();
        let home_path = Path::new(&home_env);
        let cfg = Config {
            git: Git {
                index_path: home_path.join(".mirror/index.git"),
                working_path: home_path.join(".mirror/work.git"),
                upstream_url: CRATES_IO_INDEX_URL.to_string(),
            },
            crates: Crates {
                storage_path: home_path.join(".mirror/crates"),
                upstream_url: CREATE_IO_URL.to_string(),
            },
            registry: Registry {
                address: format!("http://{}", local_ip.unwrap()),
                interval: Duration::hours(6).to_std().unwrap(),
                can_create_account: true,
                ldap: None,
            },
            database: Database {
                url: "mirror.registry.sqlite3.db".to_string(),
            },
        };

        cfg.save().unwrap();
        cfg
    }
}

impl Config {
    pub fn new() -> anyhow::Result<Self> {
        Ok(if Path::new(CONFIG_PATH).exists() {
            Config::load()?
        } else {
            Config::default()
        })
    }

    fn load() -> anyhow::Result<Self> {
        toml::from_slice(&*fs::read(CONFIG_PATH)?).map_err(|e| {
            anyhow!(
                "config file {} corrupted: \"{}\", your may correct it or delete it",
                CONFIG_PATH,
                e
            )
        })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        fs::write(CONFIG_PATH, toml::to_string(self)?)?;
        Ok(())
    }
}

#[get("config")]
pub async fn get_config(data: web::Data<Server>, id: Identity) -> Result<impl Responder> {
    let mut cfg = serde_json::to_value(&*data.config.read().await).map_err(|e| {
        ErrorInternalServerError(format!("can not convert config to json: {:?}", e))
    })?;

    if let Some(user) = check(&id, &*data.database.lock().await).ok() {
        if user.is_admin() {
            let secs = cfg["registry"]["interval"]["secs"].as_u64().unwrap();
            let duration = if secs >= 60 * 60 * 24 {
                format!("{}d", secs / (60 * 60 * 24))
            } else if secs >= 60 * 60 {
                format!("{}h", secs / (60 * 60))
            } else {
                format!("{}m", secs / 60)
            };

            cfg["registry"]["interval"] = json!(duration);
            cfg["busy"] = json!(*data.git.busy.lock().await);
        }
    } else {
        let can_create_account = cfg["registry"]["can_create_account"]
            .as_bool()
            .unwrap_or(false);
        let hostname = cfg["registry"]["ldap"]["hostname"]
            .as_str()
            .map(|s| s.to_string());
        let address = cfg["registry"]["address"].as_str().map(|s| s.to_string());
        cfg = json!({});
        cfg["registry"]["can_create_account"] = json!(can_create_account);

        if let Some(hn) = hostname {
            cfg["registry"]["ldap"] = json!({ "hostname": hn });
        }

        if let Some(addr) = address {
            cfg["registry"]["address"] = json!(addr);
        }
    }

    cfg["inited"] = json!(*data.git.inited.lock().await);
    cfg["salt"] = json!(*SALT.lock().await);
    Ok(HttpResponse::Ok().body(cfg.to_string()))
}

fn check_and_move(old: impl AsRef<Path>, new: impl AsRef<Path>) -> anyhow::Result<()> {
    let old = old.as_ref();
    let new = new.as_ref();
    if old != new {
        fs::rename(old, new).context(format!("set config mv {:?} to {:?} failed", old, new))?;
    }

    Ok(())
}

fn modify_configs(config: &mut Config, value: &Value) -> anyhow::Result<()> {
    if let Some(git_cfg) = value["git"].as_object() {
        if let Some(index_path) = git_cfg["index_path"].as_str() {
            check_and_move(&config.git.index_path, index_path)?;
            config.git.index_path = PathBuf::from(index_path);
        }

        if let Some(working_path) = git_cfg["working_path"].as_str() {
            check_and_move(&config.git.working_path, working_path)?;
            config.git.working_path = PathBuf::from(working_path);
        }

        if let Some(upstream_url) = git_cfg["upstream_url"].as_str() {
            config.git.upstream_url = upstream_url.to_string();
        }
    }

    if let Some(crates_cfg) = value["crates"].as_object() {
        if let Some(storage_path) = crates_cfg["storage_path"].as_str() {
            check_and_move(&config.crates.storage_path, storage_path)?;
            config.crates.storage_path = PathBuf::from(storage_path);
        }

        if let Some(upstream_url) = crates_cfg["upstream_url"].as_str() {
            config.crates.upstream_url = upstream_url.to_string();
        }
    }

    if let Some(db_cfg) = value["database"].as_object() {
        if let Some(db_file) = db_cfg["url"].as_str() {
            check_and_move(&config.database.url, db_file)?;
            config.database.url = db_file.to_string();
        }
    }

    if let Some(reg_cfg) = value["registry"].as_object() {
        if let Some(address) = reg_cfg["address"].as_str() {
            config.registry.address = address.to_string();
        }

        if let Some(interval) = reg_cfg["interval"].as_str() {
            let number: i64 = (&interval[0..interval.len() - 1]).parse()?;
            let interval = match &interval[interval.len() - 1..] {
                "m" => Duration::seconds(number * 60),
                "h" => Duration::seconds(number * 60 * 60),
                "d" => Duration::seconds(number * 60 * 60 * 24),
                _ => return Err(anyhow!("unsupported interval format")),
            };

            config.registry.interval = interval.to_std()?;
        }

        if let Some(cca) = reg_cfg["can_create_account"].as_bool() {
            config.registry.can_create_account = cca;
        }

        if let Some(ldap) = reg_cfg["ldap"].as_object() {
            let hostname = ldap["hostname"].as_str().unwrap_or("").to_string();
            let base_dn = ldap["base_dn"].as_str().unwrap_or("").to_string();
            let username = ldap["username"].as_str().unwrap_or("").to_string();
            let password = ldap["password"].as_str().unwrap_or("").to_string();
            let domain = ldap["domain"].as_str().unwrap_or("").to_string();
            config.registry.ldap = Some(Ldap {
                hostname,
                base_dn,
                username,
                password,
                domain,
            });
        } else {
            config.registry.ldap = None;
        }
    }

    config.save().context("failed to save config to file")?;
    Ok(())
}

#[post("config")]
pub async fn set_config(
    value: web::Json<serde_json::Value>,
    data: web::Data<Server>,
    id: Identity,
) -> Result<impl Responder> {
    if !&check(&id, &*data.database.lock().await)?.is_admin() {
        return Ok(HttpResponse::Forbidden());
    }

    let value = value.into_inner();
    let mut config = data.config.write().await;
    modify_configs(&mut config, &value)
        .map_err(|e| ErrorInternalServerError(format!("set config failed: {:?}", e)))?;

    Ok(HttpResponse::Ok())
}

#[get("init")]
pub async fn init(data: web::Data<Server>, id: Identity) -> Result<impl Responder> {
    if !&check(&id, &*data.database.lock().await)?.is_admin() {
        return Ok(HttpResponse::Forbidden().finish());
    }

    {
        if *data.git.inited.lock().await {
            return Ok(HttpResponse::Ok().finish());
        }
    }

    {
        let mut initing = data.git.busy.lock().await;
        if *initing {
            return Ok(HttpResponse::BadRequest().body("already initialiing, please wait"));
        }

        *initing = true;
    }

    let mut busy = data.git.busy.lock().await;
    data.git.initialize().await.map_err(|e| {
        *busy = false;
        ErrorInternalServerError(format!("initialize failed: {:?}", e))
    })?;

    *busy = false;
    *data.git.inited.lock().await = true;
    Ok(HttpResponse::Ok().finish())
}
