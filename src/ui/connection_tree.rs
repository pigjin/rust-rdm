use super::*;

pub(super) struct ConnectionTreeNode {
    pub key: String,
    pub label: String,
    pub parent: Option<String>,
    pub expandable: bool,
    pub expanded: bool,
}

impl RdmApp {
    pub(super) fn update_connection_filter(&mut self, cx: &App) {
        let filter = text_input(&self.conn_filter, cx).to_lowercase();
        if filter.is_empty() {
            return;
        }
        self.groups_open = self
            .connections
            .iter()
            .filter(|group| {
                group.category == "group"
                    && group
                        .connections
                        .iter()
                        .any(|child| child.name.to_lowercase().contains(&filter))
            })
            .map(|group| group.name.clone())
            .collect();
    }
    pub(super) fn connection_tree_nodes(&self) -> Vec<ConnectionTreeNode> {
        let mut nodes = Vec::new();
        for connection in &self.connections {
            let group = connection.category == "group";
            let key = if group {
                format!("{}/", connection.name)
            } else {
                format!("/{}", connection.name)
            };
            let expanded = group && self.groups_open.contains(&connection.name);
            nodes.push(ConnectionTreeNode {
                key: key.clone(),
                label: connection.name.clone(),
                parent: None,
                expandable: group && !connection.connections.is_empty(),
                expanded,
            });
            if expanded {
                for child in &connection.connections {
                    nodes.push(ConnectionTreeNode {
                        key: format!("{}/{}", connection.name, child.name),
                        label: child.name.clone(),
                        parent: Some(key.clone()),
                        expandable: false,
                        expanded: false,
                    });
                }
            }
        }
        nodes
    }
    pub(super) fn connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "left" | "right" | "enter") {
            return;
        }
        let nodes = self.connection_tree_nodes();
        if let Some(pending) = &self.connection_pending {
            let Some(index) = nodes.iter().position(|node| &node.key == pending) else {
                return;
            };
            let node = &nodes[index];
            match key {
                "enter" => self.connection_selected = Some(node.key.clone()),
                "up" if index > 0 => self.connection_pending = Some(nodes[index - 1].key.clone()),
                "down" if index + 1 < nodes.len() => {
                    self.connection_pending = Some(nodes[index + 1].key.clone())
                }
                "left" => {
                    if node.expandable && node.expanded {
                        self.groups_open.remove(&node.label);
                    } else if let Some(parent) = &node.parent {
                        self.connection_pending = Some(parent.clone());
                    }
                }
                "right" if node.expandable => {
                    if !node.expanded {
                        self.groups_open.insert(node.label.clone());
                    } else if index + 1 < nodes.len() {
                        self.connection_pending = Some(nodes[index + 1].key.clone());
                    }
                }
                _ => {}
            }
        } else if key != "enter" {
            self.connection_pending = nodes.first().map(|node| node.key.clone());
        }
        if let Some(pending) = &self.connection_pending {
            if let Some(index) = self
                .connection_tree_nodes()
                .iter()
                .position(|node| &node.key == pending)
            {
                self.connection_scroll.scroll_to_item(index);
            }
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn connection_hover_color(&self, cx: &App) -> Hsla {
        if self.dark(cx) {
            rgba(0xffffff17).into()
        } else {
            rgb(0xf3f3f5).into()
        }
    }
}
