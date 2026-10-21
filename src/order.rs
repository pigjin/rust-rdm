use crate::model::Connection;
use anyhow::{Result, bail};

#[derive(Clone)]
pub enum Destination {
    Root,
    Before { name: String, group: bool },
    Group(String),
}

pub fn move_item(
    connections: &mut Vec<Connection>,
    name: &str,
    group: bool,
    destination: Destination,
) -> Result<()> {
    let original = connections.clone();
    let result = (|| {
        if matches!(&destination,Destination::Before{name:target,group:kind}if target==name&&*kind==group)
        {
            return Ok(());
        }
        let item = if group {
            let index = connections
                .iter()
                .position(|item| item.category == "group" && item.name == name)
                .ok_or_else(|| anyhow::anyhow!("分组不存在"))?;
            connections.remove(index)
        } else {
            fn take(items: &mut Vec<Connection>, name: &str) -> Option<Connection> {
                if let Some(index) = items
                    .iter()
                    .position(|item| item.category != "group" && item.name == name)
                {
                    return Some(items.remove(index));
                }
                items
                    .iter_mut()
                    .find_map(|item| take(&mut item.connections, name))
            }
            take(connections, name).ok_or_else(|| anyhow::anyhow!("连接不存在"))?
        };
        match destination {
            Destination::Root => connections.push(item),
            Destination::Group(name) => {
                if group {
                    bail!("分组不能嵌套");
                }
                let target = connections
                    .iter_mut()
                    .find(|item| item.category == "group" && item.name == name)
                    .ok_or_else(|| anyhow::anyhow!("目标分组不存在"))?;
                target.connections.push(item);
            }
            Destination::Before {
                name,
                group: target_group,
            } => {
                if let Some(index) = connections.iter().position(|item| {
                    item.name == name && (item.category == "group") == target_group
                }) {
                    connections.insert(index, item);
                } else if !group && !target_group {
                    let (target, index) = connections
                        .iter_mut()
                        .find_map(|parent| {
                            parent
                                .connections
                                .iter()
                                .position(|child| child.name == name)
                                .map(|index| (parent, index))
                        })
                        .ok_or_else(|| anyhow::anyhow!("目标连接不存在"))?;
                    target.connections.insert(index, item);
                } else {
                    bail!("目标位置不存在");
                }
            }
        }
        Ok(())
    })();
    if result.is_err() {
        *connections = original;
    }
    result
}
