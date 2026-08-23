use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::ValueEnum;
use serde::Serialize;
use thiserror::Error;

pub const PROFILE_ENV: &str = "IPPO_PROFILE";
pub const DATABASE_ENV: &str = "IPPO_DATABASE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Personal,
    #[cfg(debug_assertions)]
    Dev,
}

impl Profile {
    pub const fn environment(self) -> DataEnvironment {
        match self {
            Self::Personal => DataEnvironment::Personal,
            #[cfg(debug_assertions)]
            Self::Dev => DataEnvironment::Development,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            #[cfg(debug_assertions)]
            Self::Dev => "dev",
        }
    }
}

impl FromStr for Profile {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "personal" => Ok(Self::Personal),
            #[cfg(debug_assertions)]
            "dev" | "development" => Ok(Self::Dev),
            invalid => Err(ConfigError::InvalidProfile(invalid.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataEnvironment {
    Personal,
    Development,
    Test,
}

impl DataEnvironment {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Development => "development",
            Self::Test => "test",
        }
    }

    pub const fn is_personal(self) -> bool {
        matches!(self, Self::Personal)
    }
}

impl FromStr for DataEnvironment {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "personal" => Ok(Self::Personal),
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            invalid => Err(ConfigError::InvalidDatabaseEnvironment(invalid.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeOptions {
    pub profile: Option<Profile>,
    pub database: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub profile: Profile,
    pub environment: DataEnvironment,
    pub database_path: PathBuf,
    pub database_overridden: bool,
}

impl RuntimeConfig {
    pub fn resolve(options: RuntimeOptions) -> Result<Self, ConfigError> {
        let home = env::var_os("HOME").map(PathBuf::from);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let xdg_data = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        #[cfg(target_os = "windows")]
        let local_app_data = env::var_os("LOCALAPPDATA").map(PathBuf::from);

        Self::resolve_from(
            options,
            env::var_os(PROFILE_ENV),
            env::var_os(DATABASE_ENV),
            PlatformRoots {
                home,
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                xdg_data,
                #[cfg(target_os = "windows")]
                local_app_data,
            },
        )
    }

    fn resolve_from(
        options: RuntimeOptions,
        env_profile: Option<OsString>,
        env_database: Option<OsString>,
        roots: PlatformRoots,
    ) -> Result<Self, ConfigError> {
        let profile = match options.profile {
            Some(profile) => profile,
            None => match env_profile {
                Some(value) => value
                    .to_str()
                    .ok_or(ConfigError::NonUnicodeProfile)?
                    .parse()?,
                None => Profile::Personal,
            },
        };

        let database_override = options.database.or_else(|| env_database.map(PathBuf::from));
        let database_overridden = database_override.is_some();
        let database_path = match database_override {
            Some(path) => path,
            None => default_database_path(profile, &roots)?,
        };

        Ok(Self {
            profile,
            environment: profile.environment(),
            database_path,
            database_overridden,
        })
    }
}

#[derive(Debug, Clone, Default)]
struct PlatformRoots {
    home: Option<PathBuf>,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    xdg_data: Option<PathBuf>,
    #[cfg(target_os = "windows")]
    local_app_data: Option<PathBuf>,
}

fn default_database_path(profile: Profile, roots: &PlatformRoots) -> Result<PathBuf, ConfigError> {
    let root = platform_data_root(roots)?;
    let path = match profile {
        Profile::Personal => root.join("ippo.db"),
        #[cfg(debug_assertions)]
        Profile::Dev => root.join("profiles").join("dev").join("ippo.db"),
    };
    Ok(path)
}

#[cfg(target_os = "macos")]
fn platform_data_root(roots: &PlatformRoots) -> Result<PathBuf, ConfigError> {
    Ok(roots
        .home
        .as_ref()
        .ok_or(ConfigError::MissingDataDirectory)?
        .join("Library")
        .join("Application Support")
        .join("ippo"))
}

#[cfg(target_os = "windows")]
fn platform_data_root(roots: &PlatformRoots) -> Result<PathBuf, ConfigError> {
    Ok(roots
        .local_app_data
        .as_ref()
        .ok_or(ConfigError::MissingDataDirectory)?
        .join("ippo"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_data_root(roots: &PlatformRoots) -> Result<PathBuf, ConfigError> {
    if let Some(xdg_data) = &roots.xdg_data {
        return Ok(xdg_data.join("ippo"));
    }

    Ok(roots
        .home
        .as_ref()
        .ok_or(ConfigError::MissingDataDirectory)?
        .join(".local")
        .join("share")
        .join("ippo"))
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[cfg_attr(
        debug_assertions,
        error("invalid ippo profile '{0}'; expected 'personal' or 'dev'")
    )]
    #[cfg_attr(
        not(debug_assertions),
        error("invalid ippo profile '{0}'; release builds support only 'personal'")
    )]
    InvalidProfile(String),
    #[error("database contains an invalid environment identity '{0}'")]
    InvalidDatabaseEnvironment(String),
    #[error("IPPO_PROFILE is not valid Unicode")]
    NonUnicodeProfile,
    #[error("could not determine the platform application-data directory")]
    MissingDataDirectory,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> PlatformRoots {
        PlatformRoots {
            home: Some(PathBuf::from("/test-home")),
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            xdg_data: Some(PathBuf::from("/test-xdg")),
            #[cfg(target_os = "windows")]
            local_app_data: Some(PathBuf::from("C:/test-local")),
        }
    }

    #[test]
    fn explicit_profile_beats_environment_profile() {
        let config = RuntimeConfig::resolve_from(
            RuntimeOptions {
                profile: Some(Profile::Personal),
                database: None,
            },
            Some(OsString::from("dev")),
            None,
            roots(),
        )
        .expect("configuration should resolve");

        assert_eq!(config.profile, Profile::Personal);
        assert_eq!(config.environment, DataEnvironment::Personal);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn explicit_database_beats_environment_database() {
        let config = RuntimeConfig::resolve_from(
            RuntimeOptions {
                profile: Some(Profile::Dev),
                database: Some(PathBuf::from("explicit.db")),
            },
            None,
            Some(OsString::from("environment.db")),
            roots(),
        )
        .expect("configuration should resolve");

        assert_eq!(config.database_path, PathBuf::from("explicit.db"));
        assert!(config.database_overridden);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn environment_profile_selects_development_database() {
        let config = RuntimeConfig::resolve_from(
            RuntimeOptions::default(),
            Some(OsString::from("dev")),
            None,
            roots(),
        )
        .expect("configuration should resolve");

        assert_eq!(config.profile, Profile::Dev);
        assert_eq!(config.environment, DataEnvironment::Development);
        assert!(config.database_path.ends_with("profiles/dev/ippo.db"));
    }

    #[test]
    fn no_profile_defaults_to_personal() {
        let config = RuntimeConfig::resolve_from(RuntimeOptions::default(), None, None, roots())
            .expect("configuration should resolve");

        assert_eq!(config.profile, Profile::Personal);
        assert_eq!(config.environment, DataEnvironment::Personal);
        assert!(config.database_path.ends_with("ippo.db"));
    }
}
