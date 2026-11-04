use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntakeSourceKind {
    Manual,
    WatchedFolder,
    Outlook,
    ErpApi,
}

impl IntakeSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "MANUAL",
            Self::WatchedFolder => "WATCHED_FOLDER",
            Self::Outlook => "OUTLOOK",
            Self::ErpApi => "ERP_API",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntakeDocumentKind {
    Quote,
    CustomerOrder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntakeCandidate {
    pub path: PathBuf,
    pub document_kind: IntakeDocumentKind,
    pub source_kind: IntakeSourceKind,
    pub sha256: String,
    pub size: u64,
    pub modified_millis: u128,
}

pub trait DocumentIntakeSource {
    fn source_kind(&self) -> IntakeSourceKind;
    fn poll(&mut self) -> io::Result<Vec<IntakeCandidate>>;
    fn acknowledge(&mut self, sha256: &str);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    size: u64,
    modified_millis: u128,
    stable_polls: u8,
}

#[derive(Debug)]
pub struct WatchedFolderSource {
    quote_folder: Option<PathBuf>,
    order_folder: Option<PathBuf>,
    observations: BTreeMap<PathBuf, Observation>,
    acknowledged_hashes: BTreeSet<String>,
    required_stable_polls: u8,
}

impl WatchedFolderSource {
    pub fn new(quote_folder: Option<PathBuf>, order_folder: Option<PathBuf>) -> Self {
        Self {
            quote_folder,
            order_folder,
            observations: BTreeMap::new(),
            acknowledged_hashes: BTreeSet::new(),
            required_stable_polls: 2,
        }
    }

    pub fn with_known_hashes(mut self, hashes: impl IntoIterator<Item = String>) -> Self {
        self.acknowledged_hashes.extend(hashes);
        self
    }

    fn roots(&self) -> [(Option<PathBuf>, IntakeDocumentKind); 2] {
        [
            (self.quote_folder.clone(), IntakeDocumentKind::Quote),
            (self.order_folder.clone(), IntakeDocumentKind::CustomerOrder),
        ]
    }
}

impl DocumentIntakeSource for WatchedFolderSource {
    fn source_kind(&self) -> IntakeSourceKind {
        IntakeSourceKind::WatchedFolder
    }

    fn poll(&mut self) -> io::Result<Vec<IntakeCandidate>> {
        let mut candidates = Vec::new();
        let mut present = BTreeSet::new();
        for (folder, document_kind) in self.roots() {
            let Some(folder) = folder else { continue };
            let entries = match fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if !entry.file_type()?.is_file() || !is_supported(&path) {
                    continue;
                }
                present.insert(path.clone());
                let metadata = entry.metadata()?;
                let modified_millis = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |duration| duration.as_millis());
                let observation = self
                    .observations
                    .entry(path.clone())
                    .or_insert(Observation {
                        size: metadata.len(),
                        modified_millis,
                        stable_polls: 0,
                    });
                if observation.size == metadata.len()
                    && observation.modified_millis == modified_millis
                {
                    observation.stable_polls = observation.stable_polls.saturating_add(1);
                } else {
                    observation.size = metadata.len();
                    observation.modified_millis = modified_millis;
                    observation.stable_polls = 0;
                    continue;
                }
                if observation.stable_polls < self.required_stable_polls {
                    continue;
                }
                let hash = match hash_readable_file(&path) {
                    Ok(hash) => hash,
                    Err(error) if is_transient_file_error(&error) => {
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if self.acknowledged_hashes.contains(&hash) {
                    continue;
                }
                candidates.push(IntakeCandidate {
                    path,
                    document_kind,
                    source_kind: IntakeSourceKind::WatchedFolder,
                    sha256: hash,
                    size: metadata.len(),
                    modified_millis,
                });
            }
        }
        self.observations.retain(|path, _| present.contains(path));
        Ok(candidates)
    }

    fn acknowledge(&mut self, sha256: &str) {
        self.acknowledged_hashes.insert(sha256.to_owned());
    }
}

fn is_transient_file_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock | io::ErrorKind::UnexpectedEof
    ) || matches!(error.raw_os_error(), Some(32 | 33))
}

pub fn manual_candidate(
    path: impl Into<PathBuf>,
    document_kind: IntakeDocumentKind,
) -> io::Result<IntakeCandidate> {
    let path = path.into();
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || !is_supported(&path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only PDF and XLSX files are supported",
        ));
    }
    let modified_millis = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis());
    Ok(IntakeCandidate {
        sha256: hash_readable_file(&path)?,
        path,
        document_kind,
        source_kind: IntakeSourceKind::Manual,
        size: metadata.len(),
        modified_millis,
    })
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("pdf") || extension.eq_ignore_ascii_case("xlsx")
        })
}

fn hash_readable_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "commercial-preflight-intake-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("temporary directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn waits_for_two_stable_observations_and_ignores_subdirectories() {
        let directory = TestDirectory::new();
        fs::write(directory.0.join("order.pdf"), b"stable bytes").expect("fixture");
        fs::create_dir(directory.0.join("nested")).expect("nested");
        fs::write(directory.0.join("nested").join("ignored.pdf"), b"ignored").expect("nested file");
        let mut source = WatchedFolderSource::new(None, Some(directory.0.clone()));
        assert!(source.poll().expect("first poll").is_empty());
        assert_eq!(source.poll().expect("second poll").len(), 1);
    }

    #[test]
    fn changing_file_resets_stability_and_identical_hash_is_deduplicated() {
        let directory = TestDirectory::new();
        let path = directory.0.join("order.xlsx");
        fs::write(&path, b"partial").expect("partial");
        let mut source = WatchedFolderSource::new(None, Some(directory.0.clone()));
        assert!(source.poll().expect("poll").is_empty());
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open");
        file.write_all(b" complete").expect("append");
        drop(file);
        assert!(source.poll().expect("changed poll").is_empty());
        assert!(source.poll().expect("first stable poll").is_empty());
        let candidates = source.poll().expect("stable poll");
        assert_eq!(candidates.len(), 1);
        source.acknowledge(&candidates[0].sha256);
        assert!(source.poll().expect("dedupe poll").is_empty());
        fs::write(directory.0.join("copy.xlsx"), b"partial complete").expect("copy");
        assert!(source.poll().expect("copy first").is_empty());
        assert!(source.poll().expect("copy second").is_empty());
    }

    #[test]
    fn manual_source_rejects_unsupported_files() {
        let directory = TestDirectory::new();
        let path = directory.0.join("order.docx");
        fs::write(&path, b"unsupported").expect("fixture");
        assert_eq!(
            manual_candidate(path, IntakeDocumentKind::CustomerOrder)
                .expect_err("unsupported")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(windows)]
    #[test]
    fn exclusively_locked_file_is_deferred_without_failure() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = TestDirectory::new();
        let path = directory.0.join("locked.pdf");
        fs::write(&path, b"locked bytes").expect("fixture");
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .expect("exclusive handle");
        let mut source = WatchedFolderSource::new(None, Some(directory.0.clone()));
        assert!(source.poll().expect("first poll").is_empty());
        assert!(source.poll().expect("locked poll").is_empty());
        drop(locked);
        assert_eq!(source.poll().expect("unlocked poll").len(), 1);
    }
}
