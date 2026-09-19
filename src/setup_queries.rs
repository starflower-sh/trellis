use sqlx::{Pool, Postgres};
use colored::Colorize;

pub async fn check_role_exists(
    pool: &Pool<Postgres>,
    user: &str,
) -> Result<bool, sqlx::Error> {
    let mut conn = pool.acquire().await?;

    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT
            EXISTS (
                SELECT
                    1
                FROM
                    pg_roles
                WHERE
                    rolname = $1
            );
        "#,
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?;

    return Ok(exists);
}

pub async fn create_database_role(
    pool: &Pool<Postgres>,
    role: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let exists = check_role_exists(pool, role).await?;

    if !exists {
        println!("Creating database role {}", role.cyan());

        let mut conn = pool.acquire().await?;

        let statement: String = sqlx::query_scalar(
            r#"
            SELECT format(
                'CREATE ROLE %I WITH PASSWORD %L',
                $1::text,
                $2::text
            );
            "#,
        )
        .bind(role)
        .bind(password)
        .fetch_one(&mut *conn)
        .await?;

        sqlx::raw_sql(&statement)
            .execute(&mut *conn)
            .await?;
    } else {
        println!("Database role {} already exists; skipping creation", role.cyan());
    }

    Ok(())
}

pub async fn set_database_role_login(
    pool: &Pool<Postgres>,
    user: &str,
    grant_login: bool,
) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;

    println!(
        "{} login for database role {}",
        if grant_login { "Enabling" } else { "Disabling" }.cyan(),
        user.cyan()
    );

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT format(
            'ALTER ROLE %I %s',
            $1::text,
            CASE WHEN $2::boolean THEN 'LOGIN' ELSE 'NOLOGIN' END
        );
        "#,
    )
    .bind(user)
    .bind(grant_login)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn create_database(
    pool: &Pool<Postgres>,
    db_name: &str,
    db_owner: &str,
) -> Result<(), sqlx::Error> {
    println!("Creating database {} with owner {}", db_name.cyan(), db_owner.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT
            format(
                'CREATE DATABASE %I OWNER %I',
                $1::text,
                $2::text
            );
        "#,
    )
    .bind(db_name)
    .bind(db_owner)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

pub async fn assign_db_ownership(
    pool: &Pool<Postgres>,
    db_name: &str,
    db_owner: &str,
) -> Result<(), sqlx::Error> {
    println!("Setting owner of database {} as {}", db_name.cyan(), db_owner.cyan());

    let mut conn = pool.acquire().await?;

    let statement: String = sqlx::query_scalar(
        r#"
        SELECT format(
            'ALTER DATABASE %I OWNER TO %I',
            $1::text,
            $2::text
        );
        "#,
    )
    .bind(db_name)
    .bind(db_owner)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::raw_sql(&statement)
        .execute(&mut *conn)
        .await?;

    Ok(())
}
