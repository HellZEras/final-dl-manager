use super::{
    errors::{File2DlError, UrlError},
    metadata::{init_metadata, MetaData},
    url::Url,
};
use futures::StreamExt;
use reqwest::{header::RANGE, redirect::Policy, Client, ClientBuilder, Error, Response};
use std::sync::atomic::Ordering::Relaxed;
use std::{
    fs::{create_dir, metadata, read_dir, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    time::{sleep, Instant},
};

#[derive(Debug, Default, Clone)]
pub struct File2Dl {
    pub url: Url,
    pub name_on_disk: String,
    pub speed: Arc<AtomicUsize>,
    pub size_on_disk: Arc<AtomicUsize>,
    pub dl_dir: String,
    pub bytes_per_sec: Arc<AtomicUsize>,
    pub running: Arc<AtomicBool>,
    pub complete: Arc<AtomicBool>,
}

impl File2Dl {
    pub async fn new(link: &str, download_path: &str) -> Result<Self, UrlError> {
        let url = Url::new(link).await?;
        if !Path::new(download_path).exists() {
            create_dir(download_path)?;
        }
        let name_on_disk = generate_name_on_disk(&url.filename, download_path)?;
        Ok(Self {
            url,
            name_on_disk,
            speed: Arc::new(AtomicUsize::new(0)),
            size_on_disk: Arc::new(AtomicUsize::new(0)),
            dl_dir: download_path.to_string(),
            bytes_per_sec: Arc::new(AtomicUsize::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            complete: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn toggle_status(&self) {
        let status = self.running.load(Relaxed);
        self.running.store(!status, Relaxed);
    }

    pub async fn single_thread_dl(&self) -> Result<(), File2DlError> {
        let client = ClientBuilder::new().redirect(Policy::limited(15)).build()?;
        let res = init_res(self, &client).await?;
        init_metadata(self, &self.dl_dir)?;
        let mut stream = res.bytes_stream();
        let file_path = Path::new(&self.dl_dir).join(&self.name_on_disk);
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)
            .await?;

        let mut accumulated_bytes = 0usize;
        let mut start_time = Instant::now();

        while let Some(packed_chunk) = stream.next().await {
            if !self.running.load(Relaxed) {
                self.bytes_per_sec.store(0, Relaxed);
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            let chunk = packed_chunk?;
            file.write_all(&chunk).await?;
            self.size_on_disk.fetch_add(chunk.len(), Relaxed);
            accumulated_bytes += chunk.len();

            if start_time.elapsed() >= Duration::from_secs(1) {
                self.bytes_per_sec.store(accumulated_bytes, Relaxed);
                accumulated_bytes = 0;
                start_time = Instant::now();
            }

            let speed_limit = self.speed.load(Relaxed);
            if speed_limit > 0 && accumulated_bytes >= speed_limit {
                let elapsed = start_time.elapsed();
                if elapsed < Duration::from_secs(1) {
                    sleep(Duration::from_secs(1) - elapsed).await;
                }
                accumulated_bytes = 0;
                start_time = Instant::now();
            }
        }

        self.complete.store(true, Relaxed);
        self.running.store(false, Relaxed);
        self.bytes_per_sec.store(0, Relaxed);
        Ok(())
    }
    pub fn from(dir: &str) -> Result<Vec<File2Dl>, std::io::Error> {
        get_metadata_files(dir)?
            .into_iter()
            .map(|entry| {
                let m_data: MetaData = {
                    let path = Path::new(dir).join(&entry);
                    let mut buf = String::new();
                    File::open(&path)?.read_to_string(&mut buf)?;
                    serde_json::from_str(&buf)?
                };
                let size_on_disk = {
                    let file_path = Path::new(dir).join(&m_data.name_on_disk);
                    get_file_size(&file_path)?
                };

                let f2dl = {
                    let url = Url {
                        link: m_data.link,
                        filename: m_data.url_name,
                        content_length: m_data.content_length,
                        range_support: m_data.range_support,
                    };
                    let name_on_disk = {
                        if m_data.range_support {
                            m_data.name_on_disk
                        } else {
                            generate_name_on_disk(&m_data.name_on_disk, dir)?
                        }
                    };
                    let is_complete = size_on_disk == m_data.content_length;
                    File2Dl {
                        url,
                        dl_dir: dir.to_string(),
                        speed: Arc::new(AtomicUsize::new(m_data.speed)),
                        bytes_per_sec: Arc::new(AtomicUsize::new(0)),
                        name_on_disk,
                        size_on_disk: Arc::new(AtomicUsize::new(size_on_disk)),
                        running: Arc::new(AtomicBool::new(false)),
                        complete: Arc::new(AtomicBool::new(is_complete)),
                    }
                };
                Ok(f2dl)
            })
            .collect()
    }
}

fn generate_name_on_disk(init: &str, download_path: &str) -> Result<String, std::io::Error> {
    let path = std::path::Path::new(download_path);
    let (name, ext) = {
        let file = Path::new(init);
        (
            file.file_stem().unwrap_or_default().to_string_lossy(),
            file.extension().unwrap_or_default().to_string_lossy(),
        )
    };
    let mut init = init.to_string();
    let mut idx = 1;
    while path.join(&init).exists() {
        init = format!("{name}_{idx}.{ext}");
        idx += 1;
    }
    Ok(init)
}
async fn init_res(f: &File2Dl, client: &Client) -> Result<Response, Error> {
    if f.url.range_support {
        return client
            .get(&f.url.link)
            .header(
                RANGE,
                format!(
                    "bytes={}-{}",
                    &f.size_on_disk.load(Relaxed),
                    &f.url.content_length
                ),
            )
            .send()
            .await;
    }
    client.get(&f.url.link).send().await
}

fn get_metadata_files(dir: &str) -> Result<Vec<String>, std::io::Error> {
    let collection = read_dir(dir)?
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let file_name = e.file_name().to_str().unwrap_or_default().to_string();
                if file_name.ends_with(".metadl") {
                    Some(file_name.to_string())
                } else {
                    None
                }
            })
        })
        .collect::<Vec<String>>();
    Ok(collection)
}

fn get_file_size(path: &PathBuf) -> Result<usize, std::io::Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = metadata(path)?;
        let size = metadata.size();
        Ok(size as usize)
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = metadata(path)?;
        let size = metadata.file_size();
        Ok(size as usize)
    }
}
