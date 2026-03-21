use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Thread-safe consumed txid tracker for replay protection.
///
/// Prevents the same transaction from being used to verify multiple payments.
#[derive(Debug, Clone)]
pub struct ConsumedTxids {
    inner: Arc<Mutex<HashSet<String>>>,
    file_path: Option<PathBuf>,
}

impl ConsumedTxids {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashSet::new())),
            file_path: None,
        }
    }

    /// Create a file-backed `ConsumedTxids` that loads existing txids from `path`
    /// on construction and appends each new txid to the file on insert.
    pub fn with_file(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let mut set = HashSet::new();
        if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            for line in contents.lines() {
                let txid = line.trim();
                if !txid.is_empty() {
                    set.insert(txid.to_string());
                }
            }
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(set)),
            file_path: Some(path),
        })
    }

    /// Check if a txid has already been consumed. If not, insert it.
    /// Returns `Err(ReplayError)` if the txid was already consumed.
    pub fn check_and_insert(&self, txid: &str) -> Result<(), ReplayError> {
        let mut set = self
            .inner
            .lock()
            .map_err(|_| ReplayError::LockPoisoned)?;
        if set.contains(txid) {
            return Err(ReplayError::AlreadyConsumed);
        }
        set.insert(txid.to_string());
        // Persist to file if configured.
        if let Some(ref path) = self.file_path {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{txid}");
            }
        }
        Ok(())
    }

    /// Remove a txid from the consumed set (e.g., if verification failed).
    pub fn remove(&self, txid: &str) {
        if let Ok(mut set) = self.inner.lock() {
            set.remove(txid);
        }
    }

    /// Check how many txids are tracked (for monitoring).
    pub fn len(&self) -> usize {
        self.inner.lock().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ConsumedTxids {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    AlreadyConsumed,
    LockPoisoned,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyConsumed => f.write_str("txid already consumed"),
            Self::LockPoisoned => f.write_str("lock poisoned"),
        }
    }
}

impl std::error::Error for ReplayError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_succeeds() {
        let consumed = ConsumedTxids::new();
        assert!(consumed.check_and_insert("tx1").is_ok());
        assert_eq!(consumed.len(), 1);
    }

    #[test]
    fn duplicate_insert_fails() {
        let consumed = ConsumedTxids::new();
        consumed
            .check_and_insert("tx1")
            .expect("first insert should succeed");
        let err = consumed
            .check_and_insert("tx1")
            .expect_err("duplicate should fail");
        assert_eq!(err, ReplayError::AlreadyConsumed);
    }

    #[test]
    fn different_txids_succeed() {
        let consumed = ConsumedTxids::new();
        consumed
            .check_and_insert("tx1")
            .expect("first should succeed");
        consumed
            .check_and_insert("tx2")
            .expect("second should succeed");
        assert_eq!(consumed.len(), 2);
    }

    #[test]
    fn remove_allows_reinsertion() {
        let consumed = ConsumedTxids::new();
        consumed
            .check_and_insert("tx1")
            .expect("should succeed");
        consumed.remove("tx1");
        consumed
            .check_and_insert("tx1")
            .expect("should succeed after removal");
    }

    #[test]
    fn clone_shares_state() {
        let consumed = ConsumedTxids::new();
        let clone = consumed.clone();
        consumed
            .check_and_insert("tx1")
            .expect("should succeed");
        let err = clone
            .check_and_insert("tx1")
            .expect_err("clone should see same state");
        assert_eq!(err, ReplayError::AlreadyConsumed);
    }

    #[test]
    fn is_empty_works() {
        let consumed = ConsumedTxids::new();
        assert!(consumed.is_empty());
        consumed
            .check_and_insert("tx1")
            .expect("should succeed");
        assert!(!consumed.is_empty());
    }
}
