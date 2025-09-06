


// Commented Version 1.0.0 Code for starting Code for Version 1.0.1 as it is available in bullg-core
// Once Version 1.0.1 is stable, remove the comments and delete Version 1.0.0 code.
// use anyhow::Result;
// use dashmap::DashMap;
// use heed::{Env, EnvOpenOptions};
// use serde::{de::DeserializeOwned, Serialize};
// use std::path::Path;
// use heed::types::{Bytes};

// pub enum Store {
//     LMDB { env: Env },
//     Memory { map: DashMap<String, Vec<u8>> },
// }

// impl Store {
//     pub fn open_lmdb<P: AsRef<Path>>(path: P) -> Result<Self> {
//         std::fs::create_dir_all(path.as_ref())?;
//         let env = unsafe { EnvOpenOptions::new().map_size(1024 * 1024 * 1024).open(path)? };
//         Ok(Self::LMDB { env })
//     }
//     pub fn memory() -> Self {
//         Self::Memory { map: DashMap::new() }
//     }

//     pub fn put<T: Serialize>(&self, db: &str, key: &str, value: &T) -> Result<()> {
//         let bytes = rmp_serde::to_vec(value)?;
//         match self {
//             Store::LMDB { env } => {
//                 let mut wtxn = env.write_txn()?;
//                 let dbi: heed::Database<Bytes, Bytes> = env.create_database(&mut wtxn, Some(db))?;
//                 dbi.put(&mut wtxn, key.as_bytes(), &bytes)?;
//                 wtxn.commit()?;
//                 Ok(())
//             }
//             Store::Memory { map } => {
//                 map.insert(format!("{}/{}", db, key), bytes);
//                 Ok(())
//             }
//         }
//     }

//     pub fn get<T: DeserializeOwned>(&self, db: &str, key: &str) -> Result<Option<T>> {
//         match self {
//             Store::LMDB { env } => {
//                 let mut wtxn = env.write_txn()?;
//                 let dbi: heed::Database<Bytes, Bytes> = env.create_database(&mut wtxn, Some(db))?;
//                 wtxn.commit()?;
//                 let r = env.read_txn()?;
//                 if let Some(bytes) = dbi.get(&r, key.as_bytes())? {
//                     Ok(Some(rmp_serde::from_slice(bytes)?))
//                 } else { Ok(None) }
//             }
//             Store::Memory { map } => {
//                 if let Some(bytes) = map.get(&format!("{}/{}", db, key)) {
//                     Ok(Some(rmp_serde::from_slice(&bytes)?))
//                 } else { Ok(None) }
//             }
//         }
//     }
// }
