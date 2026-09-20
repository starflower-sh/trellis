use std::fs::{self, OpenOptions};
use std::path::Path;

use chrono::Utc;
use postgresql_embedded::Result;

use crate::Args;

pub(crate) fn handle_create(args: &Args) -> Result<()> {
    if args.migration.is_some() {
        //TODO: Make it so the path where it creates the migration is the specified db migration
        // dir path (if it makes sense to)
        create_migration(".", &args.description)?;
    }

    Ok(())
}

pub fn create_migration(
    migrations_dir: &str,
    description: &str,
) -> Result<()> {
    let description: String = description
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                '_'
            } else {
                character
            }
        })
        .collect();

    if description.contains(['/', '\\', '\0']) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Migration descriptions cannot contain path separators or null characters",
        )
        .into());
    }

    let directory = Path::new(migrations_dir);
    fs::create_dir_all(directory)?;

    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let filename = format!("{timestamp}_{description}");
    let up_path = directory.join(format!("{filename}.up.sql"));
    let down_path = directory.join(format!("{filename}.down.sql"));

    let up_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&up_path)?;

    drop(up_file);

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&down_path)
    {
        Ok(down_file) => drop(down_file),
        Err(error) => {
            fs::remove_file(&up_path)?;
            return Err(error.into());
        }
    }

    println!("Created {}", up_path.display());
    println!("Created {}", down_path.display());

    Ok(())
}
