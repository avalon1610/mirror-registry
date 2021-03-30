//! # Mirror Registry
//! Mirror Registry is an [Alternate Registry](https://doc.rust-lang.org/cargo/reference/registries.html), can be used
//! to mirror upstream registry and serve private crates
//! # Features
//! - Mirror upstream crates.io-index 
//! - Caching download crates from crates.io (or other upstream)
//! - Support full [Registry Web API](https://doc.rust-lang.org/cargo/reference/registries.html#web-api) for private crates
//!     * cargo login   (login for publish)
//!     * cargo publish (publish private crates)
//!     * cargo yank    (can only yank private crates)
//!     * cargo search  (search from upstream and private)
//! - User-friendly Web UI
//! - User registration and login
//!     * LDAP supported
//!
//! # Prerequisites
//! Need git 2.0 or above installed
//!
//! # Install
//! ```
//! cargo install mirror-registry
//! ```
//! 
//! # Usage
//! - start the registry, input super admin username and password:
//! ```
//! ./mirror-registry
//! ```
//! - goto web ui (eg. http://localhost:55555), login with super admin
//!     * adjust the default configuration
//!     * initialize the system
//! 
//! - use it directly in cargo command:
//! ```
//! cargo search tokio --registry=http://localhost:55555/registry/crates.io-index
//! ```
//! - or setup in the ~/.cargo/config
//! ```
//! [source.crates-io]
//! replace-with = "mirror"
//! [source.mirror]
//! registry = "http://localhost:55555/registry/crates.io-index"
//! ```
//! 
//! # License
//! This project is licensed under either 
//! [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
//! or [MIT License](http://opensource.org/licenses/MIT)

mod auth;
mod config;
mod crates_io;
mod database;
mod git;
#[macro_use]
extern crate diesel_migrations;
#[macro_use]
extern crate diesel;

use crate::config::{Config, DEFAULT_PORT};
use auth::AuthContext;
use crates_io::Index;
use database::Database;
use git::Git;
use spa_server::{
    re_export::{get, HttpResponse, Responder},
    SPAServer,
};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(SPAServer)]
#[spa_server(
    static_files = "ui/dist/ui",
    apis(
        api(
            prefix = "/api/v1/crates",
            crates_io::download,
            crates_io::search,
            crates_io::publish,
            crates_io::yank,
            crates_io::unyank,
            crates_io::list_owners,
            crates_io::add_owner,
            crates_io::remove_owner,
        ),
        api(prefix = "/registry", git::http_backend_get, git::http_backend_post),
        api(
            prefix = "/web_api",
            config::get_config,
            config::set_config,
            config::init,
        ),
        api(me),
        api(
            prefix = "/auth",
            auth::who,
            auth::login,
            auth::logout,
            auth::create,
            auth::ldap_login,
            auth::modify
        )
    ),
    cors,
    identity(name = "mirror-registry-auth", age = 30)
)]
pub struct Server {
    git: Arc<Git>,
    database: Mutex<Database>,
    config: Arc<RwLock<Config>>,
    auth_context: AuthContext,
    index: Index,
}

#[get("me")]
async fn me() -> spa_server::re_export::Result<impl Responder> {
    Ok(HttpResponse::MovedPermanently().with_header(("Location", "/auth/who")))
}

#[spa_server::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config = Arc::new(RwLock::new(Config::new()?));
    let database = Database::new(config.clone()).await?;

    println!(
        "open the mirror registry web on {} for further settings",
        config.read().await.registry.address
    );
    Server {
        git: Git::new(config.clone()).await?,
        database: Mutex::new(database),
        auth_context: AuthContext::new().await?,
        index: Index::new(config.clone()),
        config,
    }
    .run(DEFAULT_PORT)
    .await?;

    Ok(())
}
