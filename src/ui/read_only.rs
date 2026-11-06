use super::*;
use dialogs::DialogKind;

pub(super) fn writes_data(kind: &DialogKind) -> bool {
    matches!(
        kind,
        DialogKind::NewKey
            | DialogKind::AddEntries
            | DialogKind::Rename
            | DialogKind::Ttl
            | DialogKind::DeleteKeys
            | DialogKind::DeleteKey(_)
            | DialogKind::Flush
            | DialogKind::Import
    )
}

pub(super) fn cli_prompt(c: &Connection, db: u32) -> String {
    let mode = if c.read_only {
        format!(" [{}]", localized("只读"))
    } else {
        String::new()
    };
    format!("{}{mode}:db{db}> ", c.name)
}

impl RdmApp {
    pub(super) fn allow_write(&mut self, c: &Connection, cx: &mut Context<Self>) -> bool {
        if let Err(error) = crate::redis::ensure_writable(c.read_only) {
            self.toast(localized(&error.to_string()), true);
            cx.notify();
            return false;
        }
        true
    }

    pub(super) fn writable_tab(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.tab_index(id) else {
            return false;
        };
        self.allow_write(&self.tabs[i].conn.clone(), cx)
    }

    pub(super) fn write_job<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        c: Connection,
        work: impl FnOnce(&Service) -> Result<T> + Send + 'static,
        apply: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let task = match self.service.write_task(&c) {
            Ok(task) => task,
            Err(error) => {
                self.toast(localized(&error.to_string()), true);
                cx.notify();
                return;
            }
        };
        self.job(
            window,
            cx,
            move |service| {
                let _task = task;
                work(service)
            },
            apply,
        );
    }
}
