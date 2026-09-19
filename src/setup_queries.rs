use sqlx::{Pool, Postgres};

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
    user: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let exists = check_role_exists(pool, user).await?;

    if !exists {
        println!("Creating database user: {user}");

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
        .bind(user)
        .bind(password)
        .fetch_one(&mut *conn)
        .await?;

        sqlx::raw_sql(&statement)
            .execute(&mut *conn)
            .await?;
    }

    Ok(())
}

pub async fn set_database_role_login(
    pool: &Pool<Postgres>,
    user: &str,
    grant_login: bool,
) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;

    println!("Setting login as {grant_login} for user: {user}");

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
    println!("Creating database: {db_name}");

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
