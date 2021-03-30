use super::models::{CrateInfo, IndexMetadata};
use crate::config::Config;
use anyhow::{anyhow, bail, Context, Result};
use log::debug;
use semver::Version;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    fs::{self, File, OpenOptions},
    io::AsyncBufReadExt,
    io::{AsyncWriteExt, BufReader},
    sync::RwLock,
};

pub struct Index {
    config: Arc<RwLock<Config>>,
}

impl Index {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Index { config }
    }

    pub async fn get_exact(
        &self,
        name: impl AsRef<str>,
        version: impl AsRef<str>,
    ) -> Result<IndexMetadata> {
        let version = version.as_ref();
        let name = name.as_ref();
        let index_file = self.get_path(name).await;
        if !index_file.exists() {
            bail!("can not found index file in work tree: {:?}", index_file);
        }

        let mut lines = BufReader::new(File::open(index_file).await?).lines();
        while let Some(line) = lines.next_line().await? {
            let meta: IndexMetadata =
                serde_json::from_str(line.trim()).context("get_exact decode metadata failed")?;
            if meta.vers == version {
                return Ok(meta);
            }
        }

        Err(anyhow!(
            "metadata not found, name: {} version: {}",
            name,
            version
        ))
    }

    async fn get_path(&self, name: &str) -> PathBuf {
        let working_path = &self.config.read().await.git.working_path;
        let index_file = match name.len() {
            1 => working_path.join("1").join(name),
            2 => working_path.join("2").join(name),
            3 => working_path.join("3").join(&name[0..1]).join(name),
            _ => working_path.join(&name[0..2]).join(&name[2..4]).join(name),
        };

        debug!("index file path: {:?}", index_file);
        index_file
    }

    pub async fn set_yank(
        &self,
        name: impl AsRef<str>,
        version: impl AsRef<str>,
        yanked: bool,
    ) -> Result<()> {
        let old_meta = self.get_exact(&name, &version).await?;
        if old_meta.yanked == yanked {
            bail!(
                "{}-{} is {}",
                name.as_ref(),
                version.as_ref(),
                if yanked {
                    "already yanked"
                } else {
                    "not yanked"
                }
            )
        }
        let mut new_meta = old_meta.clone();
        new_meta.yanked = yanked;

        let path = self.get_path(name.as_ref()).await;
        let old_data = fs::read(&path).await?;
        let old_data = String::from_utf8_lossy(&old_data);
        let new_data = old_data.replacen(
            &serde_json::to_string(&old_meta)?,
            &serde_json::to_string(&new_meta)?,
            1,
        );
        fs::write(path, new_data).await?;

        Ok(())
    }

    pub async fn append(&self, new: CrateInfo, cksum: impl Into<String>) -> Result<()> {
        let index_path = self.get_path(&new.name).await;
        let mut index_file;
        if index_path.exists() {
            index_file = OpenOptions::new()
                .read(true)
                .append(true)
                .open(index_path)
                .await?;
            let reader = BufReader::new(index_file);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                let meta: IndexMetadata = serde_json::from_str(line.trim())?;
                let old_version = Version::parse(&meta.vers)?;
                let new_version = Version::parse(&new.vers)?;
                if old_version >= new_version {
                    bail!(
                        "the new version {} is less than or equal to old version {}",
                        new.vers,
                        meta.vers,
                    );
                }
            }

            index_file = lines.into_inner().into_inner(); // get the index_file back
        } else {
            fs::create_dir_all(
                index_path
                    .parent()
                    .ok_or(anyhow!("no parent of index path: {:?}", index_path))?,
            )
            .await?;
            index_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(index_path)
                .await?;
        }

        let meta = IndexMetadata {
            name: new.name.clone(),
            vers: new.vers,
            deps: new.deps,
            cksum: cksum.into(),
            features: new.features,
            yanked: false,
            links: new.links,
        };

        index_file
            .write_all(format!("{}\n", serde_json::to_string(&meta)?).as_bytes())
            .await?;
        Ok(())
    }
}
