use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};

use crate::setup_queries::{create_database, create_database_user};

pub mod setup_queries;

#[tokio::main]
async fn main() -> Result<()> {
    let superuser = "postgres";
    let superuser_password = "postgres";
    let init_db = "postgres";

    let users = vec!["admin"];
    let user_password = "password"; // TODO: Make users a map vec
    let databases = vec!["test"];

    let data_dir = "./data";
    let host = "127.0.0.1";
    let port = 5432;
    let is_temp_db = false;

    let settings = SettingsBuilder::new()
        .host(host)
        .port(port)
        .username(superuser)
        .password(superuser_password)
        .data_dir(data_dir)
        .temporary(is_temp_db)
        .config("max_connections", "100")
        .build();

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup().await?;
    postgresql.start().await?;

    let main_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgresql.settings().url(init_db))
        .await?;

    for user in &users {
        create_database_user(&main_pool, user, user_password).await?;
    }

    for database in &databases {
        create_database(&main_pool, database, users[0]).await?;

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&postgresql.settings().url(database))
            .await?;

        let setup_result: Result<()> = async {
            create_database_user(&pool, users[0], user_password).await?;

            if !postgresql.database_exists(database).await? {
                create_database(&pool, database, user_password).await?;
            }

            Ok(())
        }
        .await;
        setup_result?;
    }

    let shutdown_result = tokio::signal::ctrl_c().await;

    shutdown_result?;
    let stop_result = postgresql.stop().await;
    stop_result
}
