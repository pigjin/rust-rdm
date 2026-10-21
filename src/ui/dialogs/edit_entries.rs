use super::*;
use crate::service::EntryUpdate;

// JavaScript parseFloat accepts a numeric prefix; JSON.stringify then turns
// NaN/Infinity into null, which the original Go float64 parameter reads as zero.
pub(super) fn score_value(text: &str) -> f64 {
    let text=text.trim_start_matches(|ch:char|matches!(ch,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'));
    for end in (1..=text.len())
        .rev()
        .filter(|end| text.is_char_boundary(*end))
    {
        let part = &text[..end];
        if part.bytes().all(|ch| b"0123456789+-.eE".contains(&ch))
            && let Ok(value) = part.parse::<f64>()
        {
            return if value.is_finite() && value != 0. {
                value
            } else {
                0.
            };
        }
    }
    0.
}

impl RdmApp {
    pub(super) fn submit_edit_entry(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let dialog = self.dialog.as_ref().unwrap();
        let DialogKind::EditEntry(original) = &dialog.kind else {
            return Ok(());
        };
        let original = original.clone();
        let target = dialog
            .target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
        let dialog_id = dialog.id;
        let pin = dialog.boolean("pin");
        let field = dialog.value("field", cx);
        let value = dialog.value("value", cx);
        let score = score_value(&dialog.value("score", cx));
        let format = dialog.value("format", cx);
        let decode = dialog.value("decode", cx);
        let selected = dialog.data["row_index"]
            .as_str()
            .unwrap_or_default()
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1));
        let current = self.tab_index(target.id).filter(|i| {
            self.tabs[*i].db == target.db && self.tabs[*i].current_key.as_ref() == Some(&target.key)
        });
        let row = current
            .and_then(|i| self.tabs[i].detail.as_ref())
            .and_then(|detail| {
                if target.kind == KeyType::Hash {
                    detail.rows.iter().find(|row| row.id == original.id)
                } else {
                    selected.and_then(|index| detail.rows.get(index))
                }
            })
            .cloned();
        let (ret_decode, ret_format) = if matches!(target.kind, KeyType::Hash | KeyType::Set) {
            current
                .map(|i| (self.tabs[i].decode.clone(), self.tabs[i].format.clone()))
                .unwrap_or_default()
        } else {
            (String::new(), String::new())
        };
        if !pin {
            self.dialog = None;
            self.full_editor = false;
        }
        let row = row.ok_or_else(|| anyhow::anyhow!("row not exists"))?;
        let submitted = if value.is_empty() && matches!(target.kind, KeyType::List | KeyType::ZSet)
        {
            original.value.clone()
        } else {
            value.as_bytes().to_vec()
        };
        let param = EntryUpdate {
            field: codec::source_string(&row.id),
            new_field: if field.is_empty() {
                codec::source_string(&original.id)
            } else {
                field
            },
            index: std::str::from_utf8(&row.id)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            value: if matches!(target.kind, KeyType::Hash | KeyType::List) {
                submitted.clone()
            } else {
                row.value
            },
            new_value: submitted.clone(),
            score,
            format: format.clone(),
            decode: decode.clone(),
            ret_format,
            ret_decode,
        };
        let custom = self.pref.decoder.clone();
        let saved = target.clone();
        let new_field = param.new_field.clone();
        self.job(
            window,
            cx,
            move |service| {
                let data = service.update_key_entry(
                    &target.conn,
                    target.db,
                    &target.key,
                    &target.kind,
                    &param,
                    &custom,
                )?;
                let converted = (target.kind == KeyType::Hash)
                    .then(|| codec::convert(&submitted, &decode, &format, &custom))
                    .transpose()?;
                Ok((data, converted, submitted))
            },
            move |app, (data, converted, submitted), w, cx| {
                if app.apply_edited_entries(&saved, &data, cx) {
                    app.reload_entry_summary(saved.clone(), w, cx);
                }
                if saved.kind == KeyType::Hash
                    && pin
                    && let Some(dialog) = app.entry_dialog_mut(dialog_id)
                    && let Some(converted) = converted
                {
                    let updated = Entry {
                        id: new_field.as_bytes().to_vec(),
                        value: submitted,
                        score: original.score,
                        ..Default::default()
                    };
                    dialog.kind = DialogKind::EditEntry(updated);
                    dialog.data["field"] = Json::String(new_field.clone());
                    let field = dialog.fields.get("field").unwrap();
                    field.update(cx, |field, cx| {
                        field.set_placeholder(new_field.clone(), w, cx)
                    });
                    set_input(field, new_field, w, cx);
                    dialog.set_converted_entry_content(converted.text, w, cx);
                }
                app.toast(app.tr("interface.save_value_succ"), false);
            },
        );
        Ok(())
    }

    pub(super) fn apply_edited_entries(
        &mut self,
        target: &Target,
        data: &Json,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(i) = self.tab_index(target.id) else {
            return false;
        };
        let tab = &mut self.tabs[i];
        if tab.db != target.db || tab.current_key.as_ref() != Some(&target.key) {
            return false;
        }
        let Some(detail) = &mut tab.detail else {
            return false;
        };
        for row in &mut detail.rows {
            if row.display_value.is_none()
                && let Some(value) = tab.row_values.get(&row.id)
            {
                row.display_value = Some(value.text.clone());
            }
        }
        let before = detail.rows.len();
        let values = |name: &str| data[name].as_array().cloned().unwrap_or_default();
        let id = |value: &Json| value.as_str().unwrap_or_default().as_bytes().to_vec();
        let parse = |value: &Json| {
            let mut entry = super::add_entries::added_entry(&target.kind, value);
            if target.kind == KeyType::List {
                entry.id = value["index"]
                    .as_i64()
                    .unwrap_or(0)
                    .to_string()
                    .into_bytes();
            }
            entry
        };
        let remove = |rows: &mut Vec<Entry>, entries: Vec<Json>| {
            for entry in entries {
                let index = if target.kind == KeyType::List {
                    entry["index"].as_u64().map(|n| n as usize)
                } else {
                    let old = id(&entry[if target.kind == KeyType::Hash {
                        "k"
                    } else {
                        "v"
                    }]);
                    rows.iter().position(|row| row.id == old)
                };
                if let Some(index) = index.filter(|index| *index < rows.len()) {
                    rows.remove(index);
                }
            }
        };
        if target.kind != KeyType::List {
            remove(&mut detail.rows, values("removed"));
        }
        for value in values("updated") {
            let entry = parse(&value);
            if let Some(row) = detail.rows.iter_mut().find(|row| row.id == entry.id) {
                *row = entry;
            } else {
                detail.rows.push(entry);
            }
        }
        detail.rows.extend(values("added").iter().map(parse));
        for value in values("replaced") {
            let index = if target.kind == KeyType::List {
                value["index"].as_u64().map(|n| n as usize)
            } else {
                let old = id(&value[if target.kind == KeyType::Hash {
                    "k"
                } else {
                    "v"
                }]);
                detail.rows.iter().position(|row| row.id == old)
            };
            let mut replacement = value.clone();
            if target.kind == KeyType::Hash {
                replacement["k"] = value["nk"].clone();
            }
            if target.kind == KeyType::ZSet {
                replacement["v"] = value["nv"].clone();
            }
            let entry = parse(&replacement);
            if let Some(index) = index.filter(|index| *index < detail.rows.len()) {
                let previous = &detail.rows[index].id;
                if tab.removed_fields.remove(previous) {
                    tab.removed_fields.insert(entry.id.clone());
                }
                detail.rows[index] = entry;
            } else if target.kind != KeyType::List || value["index"].as_i64().unwrap_or(0) >= 0 {
                detail.rows.push(entry);
            }
        }
        if target.kind == KeyType::List {
            remove(&mut detail.rows, values("removed"));
        }
        detail.length =
            (detail.length as i128 + detail.rows.len() as i128 - before as i128).max(0) as u64;
        tab.row_values.clear();
        for row in &detail.rows {
            if let Some(text) = &row.display_value {
                tab.row_values.insert(
                    row.id.clone(),
                    codec::Converted {
                        text: text.clone(),
                        decode: "None".into(),
                        format: "Raw".into(),
                    },
                );
            }
        }
        tab.entry_list
            .splice(0..tab.entry_list.item_count(), tab.entry_list.item_count());
        cx.notify();
        true
    }
}
