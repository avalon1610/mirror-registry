table! {
    accounts (id) {
        id -> Integer,
        username -> Text,
        display_name -> Text,
        salt -> Text,
        email -> Nullable<Text>,
        #[sql_name = "type"]
        type_ -> Text,
        role -> Text,
        password -> Text,
        created_at -> Timestamp,
        last_login -> Nullable<Text>,
        token -> Nullable<Text>,
    }
}

table! {
    crates (id) {
        id -> Text,
        name -> Text,
        updated_at -> Text,
        versions -> Nullable<Text>,
        keywords -> Nullable<Text>,
        categories -> Nullable<Text>,
        created_at -> Text,
        downloads -> Integer,
        recent_downloads -> Integer,
        max_version -> Text,
        newest_version -> Text,
        max_stable_version -> Nullable<Text>,
        description -> Nullable<Text>,
        homepage -> Nullable<Text>,
        documentation -> Nullable<Text>,
        repository -> Nullable<Text>,
        owners -> Nullable<Text>,
    }
}

allow_tables_to_appear_in_same_query!(
    accounts,
    crates,
);
