use anyhow::{Context, Result};
use skyline_core::{settings::AppSettings, store::Store};
use std::{path::PathBuf, thread::JoinHandle};

/// Rendering has stopped; drop the event receiver before joining a pending save.
/// An unacknowledged writer may have saved a value different from the last receipt.
pub(crate) fn flush(
    database: PathBuf,
    settings: AppSettings,
    pending: Option<JoinHandle<()>>,
) -> Result<()> {
    let worker = std::thread::Builder::new()
        .name("skyline-settings-flush".into())
        .spawn(move || {
            if let Some(pending) = pending {
                let _ = pending.join();
            }
            Store::open(&database)?.save_settings(&settings)
        })
        .context("Starting the final preferences save")?;
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("Preferences worker failed unexpectedly"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn final_save_waits_for_an_older_writer_when_user_returns_to_original_choice() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("preferences.sqlite3");
        let a = AppSettings {
            root_path: "choice-a".into(),
            ..Default::default()
        };
        let b = AppSettings {
            root_path: "choice-b".into(),
            ..Default::default()
        };
        Store::open(&database).unwrap().save_settings(&a).unwrap();
        let (release, wait) = mpsc::sync_channel(1);
        let path = database.clone();
        let pending = std::thread::spawn(move || {
            wait.recv().unwrap();
            Store::open(&path).unwrap().save_settings(&b).unwrap();
        });
        let (done, receipt) = mpsc::sync_channel(1);
        let path = database.clone();
        let final_choice = a.clone();
        let final_save = std::thread::spawn(move || {
            done.send(flush(path, final_choice, Some(pending))).unwrap();
        });
        assert!(matches!(
            receipt.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        release.send(()).unwrap();
        receipt
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        final_save.join().unwrap();
        assert_eq!(Store::open(&database).unwrap().settings().unwrap(), a);
    }

    #[test]
    fn final_save_persists_an_edit_without_a_pending_worker() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("last-edit.sqlite3");
        let settings = AppSettings {
            root_path: "last edit before quitting".into(),
            ..Default::default()
        };
        flush(database.clone(), settings.clone(), None).unwrap();
        assert_eq!(
            Store::open(&database).unwrap().settings().unwrap(),
            settings
        );
    }
}
