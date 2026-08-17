use crate::error::Error;
use crate::model::{now_ms, Profile};
use std::path::{Path, PathBuf};

const PROFILES_FILE: &str = "profiles.json";

/// JSON-backed profile store. Every method re-reads and re-writes the file,
/// so no interior mutability is required and the store is cheap to clone/hold in state.
pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(PROFILES_FILE),
        }
    }

    pub fn list(&self) -> Result<Vec<Profile>, Error> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&self.path)?;
        if bytes.trim_ascii().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn get(&self, id: &str) -> Result<Option<Profile>, Error> {
        Ok(self.list()?.into_iter().find(|p| p.id == id))
    }

    pub fn upsert(&self, mut profile: Profile) -> Result<Profile, Error> {
        let mut all = self.list()?;
        let now = now_ms();
        profile.updated_at = now;

        if profile.id.is_empty() {
            profile.id = uuid::Uuid::new_v4().to_string();
            profile.created_at = now;
            all.push(profile.clone());
        } else if let Some(existing) = all.iter_mut().find(|p| p.id == profile.id) {
            profile.created_at = existing.created_at;
            *existing = profile.clone();
        } else {
            // Unknown id from the client: treat as a new profile to avoid collisions.
            profile.id = uuid::Uuid::new_v4().to_string();
            profile.created_at = now;
            all.push(profile.clone());
        }

        self.save(&all)?;
        Ok(profile)
    }

    pub fn delete(&self, id: &str) -> Result<(), Error> {
        let mut all = self.list()?;
        let before = all.len();
        all.retain(|p| p.id != id);
        if all.len() == before {
            return Err(Error::NotFound(id.to_string()));
        }
        self.save(&all)
    }

    pub fn duplicate(&self, id: &str) -> Result<Profile, Error> {
        let mut all = self.list()?;
        let src = all
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(id.to_string()))?;

        let now = now_ms();
        let mut copy = src;
        copy.id = uuid::Uuid::new_v4().to_string();
        copy.name = format!("{} (副本)", copy.name);
        copy.created_at = now;
        copy.updated_at = now;

        all.push(copy.clone());
        self.save(&all)?;
        Ok(copy)
    }

    fn save(&self, profiles: &[Profile]) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(profiles)?;
        std::fs::write(&self.path, bytes)?;
        Ok(())
    }
}
