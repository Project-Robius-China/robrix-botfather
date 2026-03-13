use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::BotfatherState;

const DEFAULT_FILE_NAME: &str = "botfather_state.v2.json";
const LEGACY_FILE_NAMES: &[&str] = &["botfather_state.v1.json"];

#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    #[error("failed to create state directory '{0}'")]
    CreateDir(PathBuf, #[source] io::Error),
    #[error("failed to read state file '{0}'")]
    ReadFile(PathBuf, #[source] io::Error),
    #[error("failed to write state file '{0}'")]
    WriteFile(PathBuf, #[source] io::Error),
    #[error("failed to parse state file '{0}'")]
    ParseFile(PathBuf, #[source] serde_json::Error),
    #[error("failed to serialize state file '{0}'")]
    SerializeFile(PathBuf, #[source] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        Self::new(dir.as_ref().join(DEFAULT_FILE_NAME))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<BotfatherState, StateStoreError> {
        for candidate in self.load_candidates() {
            match fs::read(&candidate) {
                Ok(bytes) => match serde_json::from_slice::<BotfatherState>(&bytes) {
                    Ok(mut state) => {
                        state.normalize();
                        return Ok(state);
                    }
                    Err(error) => {
                        self.backup_invalid_state_file(&candidate, &bytes);
                        return Ok(BotfatherState::default()).inspect(|_| {
                            eprintln!(
                                "Recovered invalid BotFather state by resetting to defaults: {error}"
                            )
                        });
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StateStoreError::ReadFile(candidate, error)),
            }
        }
        Ok(BotfatherState::default())
    }

    pub fn save(&self, state: &BotfatherState) -> Result<(), StateStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| StateStoreError::CreateDir(parent.to_path_buf(), error))?;
        }
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| StateStoreError::SerializeFile(self.path.clone(), error))?;
        fs::write(&self.path, bytes)
            .map_err(|error| StateStoreError::WriteFile(self.path.clone(), error))
    }

    fn load_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = vec![self.path.clone()];
        let Some(dir) = self.path.parent() else {
            return candidates;
        };
        for legacy_name in LEGACY_FILE_NAMES {
            let legacy_path = dir.join(legacy_name);
            if legacy_path != self.path {
                candidates.push(legacy_path);
            }
        }
        candidates
    }

    fn backup_invalid_state_file(&self, source_path: &Path, bytes: &[u8]) {
        let backup_path = invalid_backup_path(source_path);
        if let Some(parent) = backup_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&backup_path, bytes);
    }
}

fn invalid_backup_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("state");
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");
    path.with_file_name(format!("{stem}.invalid-{timestamp}.{extension}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::StateStore;

    #[test]
    fn missing_file_returns_default_state() {
        let temp = tempdir().unwrap();
        let store = StateStore::in_dir(temp.path());
        let state = store.load_or_default().unwrap();
        assert!(state.bots.is_empty());
        assert!(state.runtime_profiles.is_empty());
    }

    #[test]
    fn invalid_file_is_backed_up_and_reset() {
        let temp = tempdir().unwrap();
        let store = StateStore::in_dir(temp.path());
        fs::write(store.path(), b"{ invalid json").unwrap();

        let state = store.load_or_default().unwrap();

        assert!(state.bots.is_empty());
        let backups = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".invalid-"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn legacy_v1_file_is_loaded_and_migrated() {
        let temp = tempdir().unwrap();
        let legacy_path = temp.path().join("botfather_state.v1.json");
        fs::write(
            &legacy_path,
            r#"{
              "version": 1,
              "runtime": {
                "active_sessions": {
                  "legacy": {
                    "key": {
                      "room_id": "!room:example.org",
                      "thread_root_event_id": "$thread:example.org",
                      "bot_id": "crew"
                    },
                    "runtime_profile_id": "crew-runtime",
                    "session_id": "session-1"
                  }
                }
              }
            }"#,
        )
        .unwrap();

        let store = StateStore::in_dir(temp.path());
        let state = store.load_or_default().unwrap();

        assert_eq!(state.version, crate::state::STATE_VERSION);
        assert_eq!(state.runtime.active_sessions.len(), 1);
    }
}
