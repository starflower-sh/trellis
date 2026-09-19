use postgresql_embedded::{PostgreSQL, Result, SettingsBuilder};

use crate::setup_queries::{create_database, create_database_user};

pub mod setup_queries;

#[tokio::main]
async fn main() -> Result<()> {
    let bootstrap_admin_user = "postgres";
    let bootstrap_db = "postgres";

    let admin_user = "admin";
    let admin_password = "password";
    let db_name = "test";
    let data_dir = "./data";
    let host = "127.0.0.1";
    let port = 5432;
    let is_temp_db = false;

    let settings = SettingsBuilder::new()
        .host(host)
        .port(port)
        .username(admin_user)
        .password(admin_password)
        .data_dir(data_dir)
        .temporary(is_temp_db)
        .config("max_connections", "100")
        .build();

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup().await?;
    postgresql.start().await?;

    let result: Result<()> = async {
        let mut bootstrap_settings = postgresql.settings().clone();
        bootstrap_settings.username = bootstrap_admin_user.to_owned();

        let bootstrap_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&bootstrap_settings.url(bootstrap_db))
            .await?;

        let setup_result: Result<()> = async {
            create_database_user(&bootstrap_pool, admin_user, admin_password).await?;

            if !postgresql.database_exists(db_name).await? {
                create_database(&bootstrap_pool, db_name, admin_user).await?;
            }

            Ok(())
        }
        .await;

        bootstrap_pool.close().await;
        setup_result?;

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&postgresql.settings().url(db_name))
            .await?;

        let shutdown_result = tokio::signal::ctrl_c().await;
        pool.close().await;
        shutdown_result?;

        Ok(())
    }
    .await;

    let stop_result = postgresql.stop().await;
    result?;
    stop_result
}
