use anyhow::Result;
use dashmap::DashMap;
use heed::types::Bytes;
use heed::{Env, EnvOpenOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::path::Path;
use serde_json::Value;
use regex::Regex;

pub struct Memory {
    kind: MemoryKind,
}

enum MemoryKind {
    LMDB {
        env: Env,
        dbs: DashMap<String, heed::Database<Bytes, Bytes>>,
    },
    Memory {
        map: DashMap<String, Vec<u8>>,
    },
}

impl Memory {
    /// Open LMDB storage at given path
    pub fn open_lmdb<P: AsRef<Path>>(path: P) -> Result<Self> {
        std::fs::create_dir_all(path.as_ref())?;
        let env = unsafe {
            EnvOpenOptions::new()
                .max_dbs(128)
                .map_size(1024 * 1024 * 1024 * 128)
                .open(path)?
        };
        Ok(Self {
            kind: MemoryKind::LMDB {
                env,
                dbs: DashMap::new(),
            },
        })
    }

    /// Open in-memory storage
    pub fn memory() -> Self {
        Self {
            kind: MemoryKind::Memory {
                map: DashMap::new(),
            },
        }
    }

    fn make_key(db: &str, key: &str) -> String {
        format!("{}/{}", db, key)
    }

    fn get_db<'a>(
        env: &'a Env,
        dbs: &'a DashMap<String, heed::Database<Bytes, Bytes>>,
        db_name: &str,
    ) -> Result<heed::Database<Bytes, Bytes>> {
        if let Some(dbi) = dbs.get(db_name) {
            Ok(*dbi)
        } else {
            let mut wtxn = env.write_txn()?;
            let dbi: heed::Database<Bytes, Bytes> =
                env.create_database::<Bytes, Bytes>(&mut wtxn, Some(db_name))?;
            wtxn.commit()?;
            dbs.insert(db_name.to_string(), dbi);
            Ok(dbi)
        }
    }

    /// Add new record (fails if exists)
    pub fn add<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        if self.exists(db, key)? {
            anyhow::bail!("Key already exists");
        }
        self.put(db, key, value)
    }

    /// Insert or update (upsert)
    pub fn put<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        let bytes = rmp_serde::to_vec(value)?;
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let mut wtxn = env.write_txn()?;
                dbi.put(&mut wtxn, key.as_bytes(), &bytes)?;
                wtxn.commit()?;
                Ok(())
            }
            MemoryKind::Memory { map } => {
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
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let rtxn = env.read_txn()?;
                if let Some(bytes) = dbi.get(&rtxn, key.as_bytes())? {
                    Ok(Some(rmp_serde::from_slice(bytes)?))
                } else {
                    Ok(None)
                }
            }
            MemoryKind::Memory { map } => {
                Ok(map
                    .get(&Self::make_key(db, key))
                    .map(|v| rmp_serde::from_slice(&v).unwrap()))
            }
        }
    }

    /// Delete by key
    pub fn delete(&self, db: &str, key: &str) -> Result<()> {
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let mut wtxn = env.write_txn()?;
                dbi.delete(&mut wtxn, key.as_bytes())?;
                wtxn.commit()?;
                Ok(())
            }
            MemoryKind::Memory { map } => {
                map.remove(&Self::make_key(db, key));
                Ok(())
            }
        }
    }

    /// Check if key exists
    pub fn exists(&self, db: &str, key: &str) -> Result<bool> {
        Ok(self.get::<serde_json::Value>(db, key)?.is_some())
    }

    /// Insert multiple key/value pairs at once
    pub fn insert_many<T, I>(&self, db: &str, entries: I) -> Result<()>
    where
        T: Serialize,
        I: IntoIterator<Item = (String, T)>,
    {
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let mut wtxn = env.write_txn()?;
                for (key, value) in entries {
                    let bytes = rmp_serde::to_vec(&value)?;
                    dbi.put(&mut wtxn, key.as_bytes(), &bytes)?;
                }
                wtxn.commit()?;
                Ok(())
            }
            MemoryKind::Memory { map } => {
                for (key, value) in entries {
                    let bytes = rmp_serde::to_vec(&value)?;
                    map.insert(Self::make_key(db, &key), bytes);
                }
                Ok(())
            }
        }
    }

    /// Convenience: insert from HashMap
    pub fn insert_map<I,T>(&self, db: &str, map_in: I) -> Result<()>
    where
        I: IntoIterator<Item = (String, T)>,
        T: Serialize,
    {
        self.insert_many(db, map_in.into_iter())
    }

    /// Batch insert (optimized)
    pub fn put_many<T: Serialize>(&self, db: &str, items: &[(String, T)]) -> Result<()> {
        self.insert_many(db, items.iter().map(|(k, v)| (k.clone(), v)))
    }

    /// Patch JSON fields by key
    pub fn patch(&self, db: &str, key: &str, updates: &[(String, Value)]) -> Result<()> {
        let mut record: Option<Value> = self.get(db, key)?;
        let mut obj = match record.take() {
            Some(Value::Object(m)) => m,
            Some(_) => anyhow::bail!("Cannot patch non-object value"),
            None => anyhow::bail!("Key `{}` not found", key),
        };
        for (field, val) in updates {
            obj.insert(field.clone(), val.clone());
        }
        self.put(db, key, &Value::Object(obj))
    }

    /// Get raw bytes
    pub fn get_raw(&self, db: &str, key: &str) -> Result<Option<Vec<u8>>> {
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let rtxn = env.read_txn()?;
                Ok(dbi.get(&rtxn, key.as_bytes())?.map(|b| b.to_vec()))
            }
            MemoryKind::Memory { map } => Ok(map.get(&Self::make_key(db, key)).map(|v| v.clone())),
        }
    }

    /// Replace entire object at key
    pub fn replace<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
        self.update(db, key, value)
    }

    /// Bulk delete by keys
    pub fn delete_many(&self, db: &str, keys: &[String]) -> Result<()> {
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let mut wtxn = env.write_txn()?;
                for key in keys {
                    dbi.delete(&mut wtxn, key.as_bytes())?;
                }
                wtxn.commit()?;
                Ok(())
            }
            MemoryKind::Memory { map } => {
                for key in keys {
                    map.remove(&Self::make_key(db, key));
                }
                Ok(())
            }
        }
    }

    /// Filter records by predicate
    pub fn filter<T, F>(&self, db: &str, mut pred: F) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
        F: FnMut(&T) -> bool,
    {
        Ok(self.all(db)?.into_iter().filter(|x| pred(x)).collect())
    }

    /// Get all records
    pub fn all<T: DeserializeOwned>(&self, db: &str) -> Result<Vec<T>> {
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let rtxn = env.read_txn()?;
                let mut result = Vec::new();
                for item in dbi.iter(&rtxn)? {
                    let (_k, v) = item?;
                    result.push(rmp_serde::from_slice(v)?);
                }
                Ok(result)
            }
            MemoryKind::Memory { map } => {
                let mut result = Vec::new();
                for v in map.iter() {
                    if v.key().starts_with(&format!("{}/", db)) {
                        result.push(rmp_serde::from_slice(&v.value())?);
                    }
                }
                Ok(result)
            }
        }
    }

    /// Filter JSON values
    pub fn filter_json<F>(&self, db: &str, mut pred: F) -> Result<Vec<Value>>
    where
        F: FnMut(&Value) -> bool,
    {
        Ok(self.all::<Value>(db)?.into_iter().filter(|x| pred(x)).collect())
    }

    /// Get multiple keys
    pub fn get_many<T: DeserializeOwned>(&self, db: &str, keys: &[String]) -> Result<Vec<Option<T>>> {
        let mut result = Vec::with_capacity(keys.len());
        for key in keys {
            result.push(self.get(db, key)?);
        }
        Ok(result)
    }

    /// Get multiple keys as HashMap
    pub fn get_map<T: DeserializeOwned>(&self, db: &str, keys: &[String]) -> Result<HashMap<String, T>> {
        let mut map_out = HashMap::with_capacity(keys.len());
        for key in keys {
            if let Some(val) = self.get(db, key)? {
                map_out.insert(key.clone(), val);
            }
        }
        Ok(map_out)
    }

    /// Get all as HashMap
    pub fn all_map<T: DeserializeOwned>(&self, db: &str) -> Result<HashMap<String, T>> {
        let mut map_out = HashMap::new();
        match &self.kind {
            MemoryKind::LMDB { env, dbs } => {
                let dbi = Self::get_db(env, dbs, db)?;
                let rtxn = env.read_txn()?;
                for item in dbi.iter(&rtxn)? {
                    let (k, v) = item?;
                    let key = String::from_utf8_lossy(k).to_string();
                    map_out.insert(key, rmp_serde::from_slice(v)?);
                }
            }
            MemoryKind::Memory { map } => {
                for v in map.iter() {
                    if v.key().starts_with(&format!("{}/", db)) {
                        let key = v.key().replacen(&format!("{}/", db), "", 1);
                        map_out.insert(key, rmp_serde::from_slice(&v.value())?);
                    }
                }
            }
        }
        Ok(map_out)
    }

    /// Match helper (wildcard/regex/substring)
    fn match_key(pattern: &str, candidate: &str) -> bool {
        if pattern.contains('*') {
            let regex_str = format!("^{}$", regex::escape(pattern).replace("\\*", ".*"));
            Regex::new(&regex_str).map(|r| r.is_match(candidate)).unwrap_or(false)
        } else if pattern.starts_with("regex:") {
            let expr = &pattern["regex:".len()..];
            Regex::new(expr).map(|r| r.is_match(candidate)).unwrap_or(false)
        } else if pattern.contains(candidate) || candidate.contains(pattern) {
            true
        } else {
            candidate == pattern
        }
    }

    fn longest_prefix_match<'a>(pattern: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
        let mut best: Option<String> = None;
        let mut best_len = 0;
        for c in candidates {
            if pattern.starts_with(c) && c.len() > best_len {
                best = Some(c.to_string());
                best_len = c.len();
            }
        }
        best
    }

    /// Get values by patterns
    pub fn get_filter_map<T: DeserializeOwned + Clone>(&self, db: &str, patterns: Vec<String>) -> Result<HashMap<String, T>> {
        let all_keys = self.all_map::<T>(db)?;
        let mut result = HashMap::new();
        for pattern in patterns {
            // Exact
            if let Some(val) = all_keys.get(&pattern) {
                result.insert(pattern.clone(), val.clone());
                continue;
            }
            // Wildcard / regex
            for (k, v) in &all_keys {
                if Self::match_key(&pattern, k) {
                    result.insert(k.clone(), v.clone());
                }
            }
            // Longest prefix
            if let Some(best) = Self::longest_prefix_match(&pattern, all_keys.keys().map(|s| s.as_str())) {
                if let Some(val) = all_keys.get(&best) {
                    result.insert(best.clone(), val.clone());
                }
            }
        }
        Ok(result)
    }
}