use super::*;
use crate::codec;
use serde_json::{Value as Json, json};

/// Shared fields of the original SetHash/SetList/SetSet/SetZSet RPC parameters.
#[derive(Clone, Debug, Default)]
pub struct EntryUpdate {
    pub field: String,
    pub new_field: String,
    pub index: i64,
    pub value: Vec<u8>,
    pub new_value: Vec<u8>,
    pub score: f64,
    pub format: String,
    pub decode: String,
    pub ret_format: String,
    pub ret_decode: String,
}

impl Service {
    pub fn update_key_entry(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        kind: &KeyType,
        param: &EntryUpdate,
        decoders: &[Decoder],
    ) -> Result<Json> {
        let mut client = self.client(c, db)?;
        let mut run = |command: &str, args: Vec<Vec<u8>>| {
            let mut all = vec![command.as_bytes().to_vec(), key.to_vec()];
            all.extend(args);
            self.logged(c, db, std::slice::from_ref(&all), || client.query(&all))
        };
        let save = |value: &[u8]| {
            codec::save_as(value, &param.decode, &param.format, decoders)
                .map_err(|error| anyhow::anyhow!("save to type \"{}\" fail: {error}", param.format))
        };
        let display = |value: &[u8], always: bool| {
            if !always && (param.ret_decode.is_empty() || param.ret_format.is_empty()) {
                return String::new();
            }
            if param.ret_decode == "None" && param.ret_format == "Raw" {
                return codec::source_string(value);
            }
            codec::convert(value, &param.ret_decode, &param.ret_format, decoders)
                .map(|v| v.text)
                .unwrap_or_default()
        };
        let string = |value: &[u8]| Json::String(codec::source_string(value));
        let mut data = json!({});
        let mut push = |name: &str, mut row: Json, dv: Option<String>| {
            if let Some(dv) = dv.filter(|value| !value.is_empty()) {
                row["dv"] = json!(dv);
            }
            data.as_object_mut()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(row);
        };
        match kind {
            KeyType::Hash => {
                let value = save(&param.value)?;
                let dv = display(&value, false);
                let old = param.field.as_bytes().to_vec();
                let new = param.new_field.as_bytes().to_vec();
                if new.is_empty() {
                    run("HDEL", vec![old])?;
                    push("removed", json!({"k":param.field,"v":null}), None);
                } else if old.is_empty() || old == new {
                    let n = run("HSET", vec![new, value.clone()])?.integer()?;
                    push(
                        if n > 0 { "added" } else { "updated" },
                        json!({"k":param.new_field,"v":string(&value)}),
                        Some(dv),
                    );
                } else {
                    let n = run("HSET", vec![new, value.clone()])?.integer()?;
                    run("HDEL", vec![old]).map_err(|error| anyhow::anyhow!("新值已写入，但旧字段删除失败；请刷新确认：{error}"))?;
                    if n > 0 {
                        push(
                            "replaced",
                            json!({"k":param.field,"nk":param.new_field,"v":string(&value)}),
                            Some(dv),
                        );
                    } else {
                        push("removed", json!({"k":param.field,"v":null}), None);
                        push(
                            "updated",
                            json!({"k":param.new_field,"v":string(&value)}),
                            Some(dv),
                        );
                    }
                }
            }
            KeyType::List => {
                let index = param.index.to_string().into_bytes();
                if param.value.is_empty() {
                    let marker = b"---VALUE_REMOVED_BY_TINY_RDM---".to_vec();
                    run("LSET", vec![index, marker.clone()])?;
                    run("LREM", vec![b"1".to_vec(), marker])?;
                    push("removed", json!({"index":param.index}), None);
                } else {
                    let value = save(&param.value)?;
                    run("LSET", vec![index, value.clone()])?;
                    push(
                        "replaced",
                        json!({"index":param.index,"v":string(&value)}),
                        Some(display(&value, false)),
                    );
                }
            }
            KeyType::Set => {
                let value = save(&param.new_value)?;
                let added = run("SADD", vec![value.clone()])?.integer()?;
                if value != param.value && run("SREM", vec![param.value.clone()])
                    .map_err(|error| anyhow::anyhow!("新值已写入，但旧成员删除失败；请刷新确认：{error}"))?.integer()? > 0
                {
                    push("removed", json!({"v":string(&param.value)}), None);
                }
                if added > 0 {
                    push("added", json!({"v":string(&value)}), Some(display(&value, false)));
                }
            }
            KeyType::ZSet => {
                if param.new_value.is_empty() {
                    if run("ZREM", vec![param.value.clone()])?.integer()? > 0 {
                        push("removed", json!({"s":0,"v":string(&param.value)}), None);
                    }
                } else {
                    if param.score.is_nan() { bail!("invalid score: NaN"); }
                    let value = save(&param.new_value)?;
                    let score = param.score.to_string().into_bytes();
                    if value == param.value {
                        let n = run("ZADD", vec![score, value.clone()])?.integer()?;
                        push(
                            if n > 0 { "added" } else { "updated" },
                            json!({"s":param.score,"v":string(&value)}),
                            Some(display(&value, true)),
                        );
                    } else {
                        let n = run("ZADD", vec![score, value.clone()])?.integer()?;
                        run("ZREM", vec![param.value.clone()]).map_err(|error| anyhow::anyhow!("新值已写入，但旧成员删除失败；请刷新确认：{error}"))?;
                        let dv = display(&value, true);
                        if n > 0 {
                            push(
                                "replaced",
                                json!({"s":param.score,"v":string(&param.value),"nv":string(&value)}),
                                Some(dv),
                            );
                        } else {
                            push("removed", json!({"s":0,"v":string(&param.value)}), None);
                            push(
                                "updated",
                                json!({"s":param.score,"v":string(&value)}),
                                Some(dv),
                            );
                        }
                    }
                }
            }
            _ => bail!("此类型不支持编辑条目"),
        }
        Ok(data)
    }
}
