use super::*;

/// Held from submission through worker exit, even after its tab has closed.
pub struct WriteTask {
    counts: Arc<Mutex<HashMap<String, usize>>>,
    name: String,
}

impl Drop for WriteTask {
    fn drop(&mut self) {
        let mut counts = self.counts.lock();
        if let Some(count) = counts.get_mut(&self.name) {
            *count -= 1;
            if *count == 0 {
                counts.remove(&self.name);
            }
        }
    }
}

impl Service {
    pub fn write_task(&self, c: &Connection) -> Result<WriteTask> {
        redis::ensure_writable(c.read_only)?;
        *self.writes.lock().entry(c.name.clone()).or_default() += 1;
        Ok(WriteTask {
            counts: self.writes.clone(),
            name: c.name.clone(),
        })
    }

    pub fn ensure_no_writes(&self, name: &str) -> Result<()> {
        anyhow::ensure!(
            self.writes.lock().get(name).copied().unwrap_or(0) == 0,
            "此连接还有写入任务，请等待完成或取消任务后再修改连接设置"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_writes_survive_close_and_cancel_until_the_worker_exits() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::new(Store::new(dir.path()));
        let c = Connection {
            name: "writing".into(),
            ..Default::default()
        };
        let queued = service.write_task(&c).unwrap();
        let running = service.write_task(&c).unwrap();
        service.close(&c.name);
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(service.ensure_no_writes(&c.name).is_err());
        assert!(service.ensure_no_writes("other").is_ok());
        drop(queued);
        assert!(service.ensure_no_writes(&c.name).is_err());
        drop(running);
        assert!(service.ensure_no_writes(&c.name).is_ok());
        assert!(
            service
                .write_task(&Connection {
                    read_only: true,
                    ..c
                })
                .is_err()
        );
    }
}
