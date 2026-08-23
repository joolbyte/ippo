use std::path::Path;

use serde::Serialize;

use crate::{config::Profile, storage::Database};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostics {
    pub profile: Profile,
    pub environment: String,
    pub database_path: String,
    pub database_overridden: bool,
    pub schema_version: i64,
}

impl Diagnostics {
    pub fn collect(
        profile: Profile,
        database_path: &Path,
        database_overridden: bool,
        database: &Database,
    ) -> Result<Self, crate::storage::DatabaseError> {
        Ok(Self {
            profile,
            environment: database.environment_identity()?.to_owned(),
            database_path: database_path.to_string_lossy().into_owned(),
            database_overridden,
            schema_version: database.schema_version()?,
        })
    }

    pub fn human_readable(&self) -> String {
        format!(
            "profile: {}\nenvironment: {}\ndatabase: {}\ndatabase override: {}\nschema version: {}",
            self.profile.as_str(),
            self.environment,
            self.database_path,
            if self.database_overridden {
                "yes"
            } else {
                "no"
            },
            self.schema_version
        )
    }
}
