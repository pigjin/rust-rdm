#[cfg(feature = "ui-qa")]
use super::views::tree_rows;
use super::views::{Node, TreeRow, build_key_tree, tree_rows_from};
use super::*;

#[derive(Default)]
pub(super) struct Cache {
    tree: RefCell<Option<TreeSnapshot>>,
    rows: RefCell<Option<Snapshot>>,
}
struct TreeSnapshot {
    revision: u64,
    source: (usize, usize),
    separator: String,
    root: Arc<Node>,
}
fn tree(tab: &Tab) -> Arc<Node> {
    let mut cache = tab.key_tree_cache.tree.borrow_mut();
    let source = (tab.keys.as_ptr() as usize, tab.keys.len());
    if let Some(old) = cache.as_ref()
        && old.revision == tab.key_revision
        && old.source == source
        && old.separator == tab.conn.key_separator
    {
        return old.root.clone();
    }
    let root = Arc::new(build_key_tree(tab));
    *cache = Some(TreeSnapshot {
        revision: tab.key_revision,
        source,
        separator: tab.conn.key_separator.clone(),
        root: root.clone(),
    });
    root
}
struct Snapshot {
    revision: u64,
    source: (usize, usize),
    db: u32,
    filter: String,
    flat: bool,
    separator: String,
    check_mode: bool,
    root_expanded: bool,
    expanded: HashSet<String>,
    rows: Arc<Vec<TreeRow>>,
}
pub(super) fn visible_rows(tab: &Tab) -> Arc<Vec<TreeRow>> {
    let mut cache = tab.key_tree_cache.rows.borrow_mut();
    let source = (tab.keys.as_ptr() as usize, tab.keys.len());
    if let Some(old) = cache.as_ref()
        && old.revision == tab.key_revision
        && old.source == source
        && old.db == tab.db
        && old.filter == tab.key_filter
        && old.flat == tab.flat
        && old.separator == tab.conn.key_separator
        && old.check_mode == tab.check_mode
        && old.root_expanded == tab.key_root_expanded
        && old.expanded == tab.expanded
    {
        return old.rows.clone();
    }
    let rows = Arc::new(tree_rows_from(tab, &tab.key_filter, false, &tree(tab)));
    *cache = Some(Snapshot {
        revision: tab.key_revision,
        source,
        db: tab.db,
        filter: tab.key_filter.clone(),
        flat: tab.flat,
        separator: tab.conn.key_separator.clone(),
        check_mode: tab.check_mode,
        root_expanded: tab.key_root_expanded,
        expanded: tab.expanded.clone(),
        rows: rows.clone(),
    });
    rows
}

#[cfg(feature = "ui-qa")]
pub(super) fn verify(tab: &mut Tab) {
    let root = tree(tab);
    assert!(Arc::ptr_eq(&root, &tree(tab)));
    let a = visible_rows(tab);
    assert!(
        Arc::ptr_eq(&a, &visible_rows(tab)),
        "unchanged renders reuse visible rows"
    );
    tab.key_revision += 1;
    assert!(!Arc::ptr_eq(&root, &tree(tab)));
    let root = tree(tab);
    let b = visible_rows(tab);
    assert!(!Arc::ptr_eq(&a, &b));
    assert_eq!(*b, tree_rows(tab, &tab.key_filter, false));
    tab.flat = !tab.flat;
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    assert!(
        Arc::ptr_eq(&root, &tree(tab)),
        "display changes reuse the tree"
    );
    tab.flat = !tab.flat;
    tab.key_filter = "no-cache-match".into();
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    tab.key_filter.clear();
    let root_expanded = tab.key_root_expanded;
    tab.key_root_expanded = !root_expanded;
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    tab.key_root_expanded = root_expanded;
    let expanded = tab.expanded.clone();
    tab.expanded.insert("test".into());
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    assert!(
        Arc::ptr_eq(&root, &tree(tab)),
        "expansion only rebuilds visible rows"
    );
    tab.expanded = expanded;
    let separator = tab.conn.key_separator.clone();
    tab.conn.key_separator = "|".into();
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    tab.conn.key_separator = separator;
    let keys = tab.keys.clone();
    tab.keys.push(KeyEntry {
        key: b"cache:added".to_vec(),
        kind: KeyType::String,
    });
    tab.key_revision += 1;
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    tab.keys.last_mut().unwrap().key = b"cache:renamed".to_vec();
    tab.key_revision += 1;
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
    tab.keys = keys;
    tab.key_revision += 1;
    assert_eq!(*visible_rows(tab), tree_rows(tab, &tab.key_filter, false));
}
