use super::account::AccountRole;
use super::account::SALT;
use crate::database::schema::accounts;

#[derive(Insertable, AsChangeset, Default, Clone)]
#[table_name = "accounts"]
pub struct Account {
    pub username: String,
    pub display_name: String,
    pub salt: String,
    pub email: Option<String>,
    pub type_: String,
    pub role: String,
    pub password: String,
    pub last_login: Option<String>,
    pub token: Option<String>,
}

#[derive(Queryable)]
pub struct AccountWithId {
    pub id: i32,
    pub username: String,
    pub display_name: String,
    pub salt: String,
    email: Option<String>,
    pub type_: String,
    pub role: String,
    pub password: String,
    #[allow(dead_code)]
    created_at: chrono::NaiveDateTime,
    pub last_login: Option<String>,
    pub token: Option<String>,
}

impl From<AccountWithId> for Account {
    fn from(a: AccountWithId) -> Self {
        Account {
            username: a.username,
            display_name: a.display_name,
            salt: a.salt,
            email: a.email,
            type_: a.type_,
            role: a.role,
            password: a.password,
            last_login: a.last_login,
            token: a.token,
        }
    }
}

impl Account {
    pub fn new(username: impl Into<String>, type_: impl Into<String>, role: impl Into<String>) -> Self {
        let username = username.into();
        Account {
            display_name: username.clone(),
            username,
            type_: type_.into(),
            role: role.into(),
            ..Default::default()
        }
    }

    pub async fn password(&mut self, pwd: impl AsRef<str>) -> &mut Self {
        let salt = SALT.lock().await;
        self.password = format!(
            "{:?}",
            md5::compute(format!("{}:{}:{}", self.username, salt, pwd.as_ref()))
        );
        self.salt = salt.clone();
        self
    }

    pub fn encoded_password(&mut self, pwd: impl Into<String>) -> &mut Self {
        self.password = pwd.into();
        self
    }

    pub fn salt(&mut self, salt: impl Into<String>) -> &mut Self {
        self.salt = salt.into();
        self
    }

    pub fn display_name(&mut self, new_value: impl Into<String>) -> &mut Self {
        self.display_name = new_value.into();
        self
    }

    pub fn email(&mut self, new_value: impl Into<String>) -> &mut Self {
        self.email = Some(new_value.into());
        self
    }

    pub fn token(&mut self, new_value: impl Into<String>) -> &mut Self {
        self.token = Some(new_value.into());
        self
    }

    pub fn last_login(&mut self, new_value: impl Into<String>) -> &mut Self {
        self.last_login = Some(new_value.into());
        self
    }

    pub fn set_role(&mut self, new_value: impl Into<String>) -> &mut Self {
        self.role = new_value.into();
        self
    }
    pub fn is_admin(&self) -> bool {
        self.type_ != AccountRole::User.as_ref()
    }
}
