//! The branch tree beside the history: the references of a repository grouped
//! into folders by `/`, and the history scope each row shows when selected.

use crate::railway::HistoryScope;
use git2::{Error, ReferenceType, Repository};
use std::cmp::Ordering;
use std::collections::BTreeMap;

const LOCAL_PREFIX: &str = "refs/heads/";
const REMOTE_PREFIX: &str = "refs/remotes/";
const TAG_PREFIX: &str = "refs/tags/";
// Listed first among the remotes, as it is usually the main one
const PREFERRED_REMOTE: &str = "origin";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchTreeKind {
    AllBranches,
    CurrentBranch,
    /// Local, Remote or Tags
    Header,
    Folder,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchTreeNode {
    pub label: String,
    pub kind: BranchTreeKind,
    pub scope: HistoryScope,
    /// True for the local branch that HEAD points to
    pub is_head: bool,
    pub children: Vec<BranchTreeNode>,
}

impl BranchTreeNode {
    fn new(label: String, kind: BranchTreeKind, scope: HistoryScope, children: Vec<BranchTreeNode>) -> BranchTreeNode {
        BranchTreeNode { label, kind, scope, is_head: false, children }
    }

    /// The branch name a checkout takes, like `feature/login` or `origin/main`.
    pub fn branch_name(&self) -> Option<&str> {
        match (&self.kind, &self.scope) {
            (BranchTreeKind::LocalBranch, HistoryScope::Ref(name)) => name.strip_prefix(LOCAL_PREFIX),
            (BranchTreeKind::RemoteBranch, HistoryScope::Ref(name)) => name.strip_prefix(REMOTE_PREFIX),
            _ => None,
        }
    }
}

/// The rows of the tree from the top: all branches, the current branch, then
/// the Local, Remote and Tags headers. A header without references is left out.
pub fn collect_branch_tree(repo: &Repository) -> Result<Vec<BranchTreeNode>, Error> {
    let mut local = Folder::default();
    let mut remote = Folder::default();
    let mut tags = Folder::default();

    for reference in repo.references()? {
        let reference = reference?;
        // Like origin/HEAD, which only points to another branch
        if reference.kind() != Some(ReferenceType::Direct) {
            continue;
        }
        let Ok(name) = reference.name() else { continue };
        if let Some(path) = name.strip_prefix(LOCAL_PREFIX) {
            local.insert(path, name);
        } else if let Some(path) = name.strip_prefix(REMOTE_PREFIX) {
            remote.insert(path, name);
        } else if let Some(path) = name.strip_prefix(TAG_PREFIX) {
            tags.insert(path, name);
        }
    }

    let head_name = head_reference_name(repo);
    let mut nodes = vec![
        BranchTreeNode::new("All Branches".to_owned(), BranchTreeKind::AllBranches, HistoryScope::AllBranches, vec![]),
        // The branch button names the branch, so the row does not
        BranchTreeNode::new(
            "Current Branch".to_owned(),
            BranchTreeKind::CurrentBranch,
            HistoryScope::CurrentBranch,
            vec![],
        ),
    ];

    if !local.is_empty() {
        let mut children = local.into_nodes(LOCAL_PREFIX, BranchTreeKind::LocalBranch, SortOrder::Ascending);
        mark_head(&mut children, head_name.as_deref());
        nodes.push(header("Local", LOCAL_PREFIX, children));
    }
    if !remote.is_empty() {
        nodes.push(header("Remote", REMOTE_PREFIX, remote.into_remote_nodes()));
    }
    if !tags.is_empty() {
        nodes.push(header(
            "Tags",
            TAG_PREFIX,
            tags.into_nodes(TAG_PREFIX, BranchTreeKind::Tag, SortOrder::Descending),
        ));
    }

    Ok(nodes)
}


/// Compares names ignoring case, and numbers by their value, so that
/// `issue-2` comes before `issue-10`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chunks = Chunks(a);
    let mut b_chunks = Chunks(b);
    loop {
        let order = match (a_chunks.next(), b_chunks.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => compare_chunks(x, y),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
}

fn compare_chunks(a: &str, b: &str) -> Ordering {
    let a_is_number = a.starts_with(|c: char| c.is_ascii_digit());
    let b_is_number = b.starts_with(|c: char| c.is_ascii_digit());
    match (a_is_number, b_is_number) {
        (true, true) => {
            let a_digits = a.trim_start_matches('0');
            let b_digits = b.trim_start_matches('0');
            a_digits.len().cmp(&b_digits.len()).then_with(|| a_digits.cmp(b_digits))
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// Splits a name into runs of digits and runs of everything else.
struct Chunks<'a>(&'a str);

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let first = self.0.chars().next()?;
        let is_digit = first.is_ascii_digit();
        let end = self
            .0
            .find(|c: char| c.is_ascii_digit() != is_digit)
            .unwrap_or(self.0.len());
        let (chunk, rest) = self.0.split_at(end);
        self.0 = rest;
        Some(chunk)
    }
}

#[derive(Clone, Copy)]
enum SortOrder {
    Ascending,
    Descending,
}

/// References under one `/`-separated folder, before they become rows.
#[derive(Default)]
struct Folder {
    folders: BTreeMap<String, Folder>,
    // Name in this folder, then the full reference name
    leaves: Vec<(String, String)>,
}

impl Folder {
    fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.leaves.is_empty()
    }

    fn insert(&mut self, path: &str, reference_name: &str) {
        match path.split_once('/') {
            Some((folder, rest)) => self
                .folders
                .entry(folder.to_owned())
                .or_default()
                .insert(rest, reference_name),
            None => self.leaves.push((path.to_owned(), reference_name.to_owned())),
        }
    }

    /// The references come before the folders, each sorted by name. A name
    /// without a folder, like `main`, is usually a long-lived branch, while
    /// work branches go into folders like `feature/`.
    fn into_nodes(self, prefix: &str, leaf_kind: BranchTreeKind, order: SortOrder) -> Vec<BranchTreeNode> {
        let mut folders = self
            .folders
            .into_iter()
            .map(|(name, folder)| folder_node(name, folder, prefix, leaf_kind, order))
            .collect::<Vec<_>>();
        let mut leaves = self
            .leaves
            .into_iter()
            .map(|(name, reference_name)| {
                BranchTreeNode::new(name, leaf_kind, HistoryScope::Ref(reference_name), vec![])
            })
            .collect::<Vec<_>>();
        sort_nodes(&mut folders, order);
        sort_nodes(&mut leaves, order);
        leaves.extend(folders);
        leaves
    }

    /// The first part of a remote branch is the remote name. Each remote keeps
    /// its own row, even with a single folder in it.
    fn into_remote_nodes(self) -> Vec<BranchTreeNode> {
        let mut remotes = self
            .folders
            .into_iter()
            .map(|(name, folder)| {
                let prefix = format!("{}{}/", REMOTE_PREFIX, name);
                let children = folder.into_nodes(&prefix, BranchTreeKind::RemoteBranch, SortOrder::Ascending);
                BranchTreeNode::new(format!("{}/", name), BranchTreeKind::Folder, HistoryScope::Prefix(prefix), children)
            })
            .collect::<Vec<_>>();
        remotes.sort_by(|a, b| {
            let a_preferred = a.label.trim_end_matches('/') == PREFERRED_REMOTE;
            let b_preferred = b.label.trim_end_matches('/') == PREFERRED_REMOTE;
            b_preferred.cmp(&a_preferred).then_with(|| natural_cmp(&a.label, &b.label))
        });

        let mut leaves = self
            .leaves
            .into_iter()
            .map(|(name, reference_name)| {
                BranchTreeNode::new(name, BranchTreeKind::RemoteBranch, HistoryScope::Ref(reference_name), vec![])
            })
            .collect::<Vec<_>>();
        sort_nodes(&mut leaves, SortOrder::Ascending);
        remotes.extend(leaves);
        remotes
    }
}

/// A folder with only one folder in it shares its row, like `user/yoichi/`.
fn folder_node(
    name: String,
    mut folder: Folder,
    prefix: &str,
    leaf_kind: BranchTreeKind,
    order: SortOrder,
) -> BranchTreeNode {
    let mut label = format!("{}/", name);
    let mut folder_prefix = format!("{}{}/", prefix, name);
    while folder.leaves.is_empty() && folder.folders.len() == 1 {
        let (child_name, child) = folder.folders.pop_first().unwrap();
        label.push_str(&child_name);
        label.push('/');
        folder_prefix.push_str(&child_name);
        folder_prefix.push('/');
        folder = child;
    }
    let children = folder.into_nodes(&folder_prefix, leaf_kind, order);
    BranchTreeNode::new(label, BranchTreeKind::Folder, HistoryScope::Prefix(folder_prefix), children)
}

fn sort_nodes(nodes: &mut [BranchTreeNode], order: SortOrder) {
    nodes.sort_by(|a, b| match order {
        SortOrder::Ascending => natural_cmp(&a.label, &b.label),
        SortOrder::Descending => natural_cmp(&b.label, &a.label),
    });
}

fn header(label: &str, prefix: &str, children: Vec<BranchTreeNode>) -> BranchTreeNode {
    BranchTreeNode::new(label.to_owned(), BranchTreeKind::Header, HistoryScope::Prefix(prefix.to_owned()), children)
}

fn mark_head(nodes: &mut [BranchTreeNode], head_name: Option<&str>) {
    for node in nodes {
        if let HistoryScope::Ref(name) = &node.scope {
            node.is_head = Some(name.as_str()) == head_name;
        }
        mark_head(&mut node.children, head_name);
    }
}

fn head_reference_name(repo: &Repository) -> Option<String> {
    let head = repo.find_reference("HEAD").ok()?;
    head.symbolic_target().ok().flatten().map(str::to_owned)
}
