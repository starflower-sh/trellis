use postgresql_embedded::Result;
use sqlx::PgPool;
use colored::Colorize;

use crate::{Args, apply::{apply_migrations, rollback_migration}, setup_queries::create_tracking_tables};


pub(crate) async fn handle_test(
    main_pool: &PgPool,
    args: &Args,
) -> Result<()> {
    if args.migration.is_some() {
        //TODO: Find db name & migration dir using config
        test_migrations(&main_pool, "test", "./migrations").await?;
    }

    Ok(())
}

async fn test_migrations(
    pool: &PgPool,
    database_name: &str,
    migrations_dir: &str,
) -> Result<()> {
    create_tracking_tables(&pool).await?;
    apply_migrations(&pool, database_name, migrations_dir).await?;
    
    loop {
        let res = rollback_migration(&pool, database_name, migrations_dir).await?;
        if res != 200 {
            break;
        }
    }
   println!("{}", "Successfully applied and rolled back all migrations".green());

    Ok(())
}
