use super::{
    models::{Account, AccountWithId},
    rand_str,
};
use crate::database::{schema::accounts::dsl::*, Database};
use anyhow::{bail, Context, Result};
use diesel::{associations::HasTable, prelude::*, SqliteConnection};
use log::{debug, warn};
use once_cell::sync::Lazy;
use std::io::{self, BufRead, Write};
use strum::AsRefStr;
use tokio::sync::Mutex;

#[allow(dead_code)]
#[derive(AsRefStr)]
pub(super) enum AccountRole {
    Root,
    Admin,
    User,
}

#[allow(dead_code)]
#[derive(AsRefStr)]
pub(super) enum AccountType {
    Internal,
    Ldap,
}

pub static SALT: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

pub async fn setup_root(conn: &SqliteConnection) -> Result<()> {
    let records = accounts
        .filter(role.eq(AccountRole::Root.as_ref()))
        .load::<AccountWithId>(conn)?;

    if records.len() > 0 {
        let mut salt_str = SALT.lock().await;
        *salt_str = records[0].salt.clone();
        debug!("user salt is {}", salt_str);
        return Ok(());
    }

    print!("input super admin username: ");
    io::stdout().flush().unwrap();
    let stdin = io::stdin();
    let mut root_name = String::new();
    stdin.lock().read_line(&mut root_name).unwrap();

    let root_pass = rpassword::prompt_password_stdout("input super admin password: ")?;
    *SALT.lock().await = rand_str(32);

    let mut account = Account::new(
        root_name.trim(),
        AccountType::Internal.as_ref(),
        AccountRole::Root.as_ref(),
    );
    account.password(root_pass).await;
    diesel::insert_into(accounts::table())
        .values(account)
        .execute(conn)?;

    Ok(())
}

pub fn get_user_by_token(db: &Database, tk: impl AsRef<str>) -> Result<Account> {
    let records = accounts
        .filter(token.eq(tk.as_ref()))
        .load::<AccountWithId>(&db.connection)?;

    match records.len() {
        1 => Ok(records.into_iter().nth(0).unwrap().into()),
        0 => bail!("invalid token {}", tk.as_ref()),
        _ => bail!(
            "more then one user has same token {}, impossible!",
            tk.as_ref()
        ),
    }
}

pub fn get_user_by_name(db: &Database, name: impl AsRef<str>) -> Result<Option<Account>> {
    let records = accounts
        .filter(username.eq(name.as_ref()))
        .load::<AccountWithId>(&db.connection)?;

    match records.len() {
        1 => Ok(Some(records.into_iter().nth(0).unwrap().into())),
        0 => {
            warn!("{} not found", name.as_ref());
            Ok(None)
        }
        _ => bail!(
            "found user {} more than once, database corrupted",
            name.as_ref()
        ),
    }
}

impl Account {
    pub fn update(&self, db: &Database) -> anyhow::Result<()> {
        diesel::update(accounts.filter(username.eq(&self.username)))
            .set(self)
            .execute(&db.connection)
            .context("save account to db failed")?;
        Ok(())
    }

    pub fn insert(&self, db: &Database) -> anyhow::Result<()> {
        diesel::insert_into(accounts::table())
            .values(self)
            .execute(&db.connection)
            .context("insert account to db failed")?;
        Ok(())
    }
}
