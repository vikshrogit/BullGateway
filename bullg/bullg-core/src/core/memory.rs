use anyhow::Result;
use dashmap::DashMap;
use heed::types::Bytes;
use heed::{Env, EnvOpenOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

/// Abstraction over LMDB or In-Memory storage
pub enum Memory {
    LMDB { env: Env },
    Memory { map: DashMap<String, Vec<u8>> },
}

impl Memory {
    /// Open LMDB storage at given path
    pub fn open_lmdb<P: AsRef<Path>>(path: P) -> Result<Self> {
        std::fs::create_dir_all(path.as_ref())?;
        let env = unsafe { EnvOpenOptions::new().map_size(1024 * 1024 * 1024).open(path)? };
        Ok(Self::LMDB { env })
    }

    /// Open in-memory storage
    pub fn memory() -> Self {
        Self::Memory { map: DashMap::new() }
    }

    /// Internal key formatting: `db/key`
    fn make_key(db: &str, key: &str) -> String {
        format!("{}/{}", db, key)
    }

    /// Add a new record (fails if key exists)
    pub fn add<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        if self.exists(db, key)? {
            anyhow::bail!("Key already exists");
        }
        self.put(db, key, value)
    }

    /// Insert or update (upsert)
    pub fn put<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        let bytes = rmp_serde::to_vec(value)?;
        match self {
            Memory::LMDB { env } => {
                let mut wtxn = env.write_txn()?;
                let dbi: heed::Database<Bytes, Bytes> = env.create_database(&mut wtxn, Some(db))?;
                dbi.put(&mut wtxn, key.as_bytes(), &bytes)?;
                wtxn.commit()?;
                Ok(())
            }
            Memory::Memory { map } => {
                map.insert(Self::make_key(db, key), bytes);
                Ok(())
            }
        }
    }

    /// Update existing record (fails if not exists)
    pub fn update<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        if !self.exists(db, key)? {
            anyhow::bail!("Key does not exist");
        }
        self.put(db, key, value)
    }

    /// Get by key
    pub fn get<T: DeserializeOwned>(&self, db: &str, key: &str) -> Result<Option<T>> {
        match self {
            Memory::LMDB { env } => {
                {
                    let mut wtxn = env.write_txn()?;
                    env.create_database::<Bytes, Bytes>(&mut wtxn, Some(db))?;
                    wtxn.commit()?;
                }

                // Now open read-only transaction to fetch data
                let mut rtxn = env.read_txn()?;
                let dbi: heed::Database<Bytes, Bytes> = env.open_database(&mut rtxn, Some(db))?
                    .ok_or_else(|| anyhow::anyhow!("DB `{}` not found", db))?;

                if let Some(bytes) = dbi.get(&rtxn, key.as_bytes())? {
                    Ok(Some(rmp_serde::from_slice(bytes)?))
                } else {
                    Ok(None)
                }
            }
            Memory::Memory { map } => {
                if let Some(bytes) = map.get(&Self::make_key(db, key)) {
                    Ok(Some(rmp_serde::from_slice(&bytes)?))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Delete by key
    pub fn delete(&self, db: &str, key: &str) -> Result<()> {
        match self {
            Memory::LMDB { env } => {
                let mut wtxn = env.write_txn()?;
                let dbi: heed::Database<Bytes, Bytes> = env.create_database(&mut wtxn, Some(db))?;
                dbi.delete(&mut wtxn, key.as_bytes())?;
                wtxn.commit()?;
                Ok(())
            }
            Memory::Memory { map } => {
                map.remove(&Self::make_key(db, key));
                Ok(())
            }
        }
    }

    /// Check if key exists
    pub fn exists(&self, db: &str, key: &str) -> Result<bool> {
        Ok(self.get::<serde_json::Value>(db, key)?.is_some())
    }

    /// Get all records from db
    pub fn all<T: DeserializeOwned>(&self, db: &str) -> Result<Vec<T>> {
        match self {
            Memory::LMDB { env } => {
                {
                    let mut wtxn = env.write_txn()?;
                    env.create_database::<Bytes, Bytes>(&mut wtxn, Some(db))?;
                    wtxn.commit()?;
                }
                let mut rtxn = env.read_txn()?;
                let dbi: heed::Database<Bytes, Bytes> = env.open_database(&mut rtxn, Some(db))?.expect("REASON");
                let mut result = Vec::new();
                for item in dbi.iter(&rtxn)? {
                    let (_k, v) = item?;
                    let val: T = rmp_serde::from_slice(v)?;
                    result.push(val);
                }
                Ok(result)
            }
            Memory::Memory { map } => {
                let mut result = Vec::new();
                for v in map.iter() {
                    if v.key().starts_with(&format!("{}/", db)) {
                        let val: T = rmp_serde::from_slice(&v.value())?;
                        result.push(val);
                    }
                }
                Ok(result)
            }
        }
    }

    /// Filter records by predicate
    pub fn filter<T, F>(&self, db: &str, mut pred: F) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
        F: FnMut(&T) -> bool,
    {
        let all_items: Vec<T> = self.all(db)?;
        Ok(all_items.into_iter().filter(|x| pred(x)).collect())
    }
}
