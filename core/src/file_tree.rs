use mei_proto::SophonManifestAssetProperty;

#[derive(Clone, Debug)]
pub struct FileNode {
    pub name: String,
    pub full_path: String,
    pub is_file: bool,
    pub size: i64,
    pub children: Vec<FileNode>,
    pub asset: Option<SophonManifestAssetProperty>,
    pub file_count: usize,
}

impl FileNode {
    pub fn new_file(
        name: String,
        full_path: String,
        size: i64,
        asset: SophonManifestAssetProperty,
    ) -> Self {
        Self {
            name,
            full_path,
            is_file: true,
            size,
            children: Vec::new(),
            asset: Some(asset),
            file_count: 1,
        }
    }

    pub fn new_folder(name: String, full_path: String) -> Self {
        Self {
            name,
            full_path,
            is_file: false,
            size: 0,
            children: Vec::new(),
            asset: None,
            file_count: 0,
        }
    }

    pub fn add_child(&mut self, child: FileNode) {
        self.size += child.size;
        self.file_count += if child.is_file { 1 } else { child.file_count };
        self.children.push(child);
    }

    pub fn sort_children(&mut self) {
        // Sort folders first, then by name
        self.children.sort_by(|a, b| match (a.is_file, b.is_file) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        for child in &mut self.children {
            child.sort_children();
        }
    }

    pub fn collect_all_files(&self) -> Vec<SophonManifestAssetProperty> {
        let mut files = Vec::new();

        if self.is_file {
            if let Some(asset) = &self.asset {
                files.push(asset.clone());
            }
        } else {
            for child in &self.children {
                files.extend(child.collect_all_files());
            }
        }

        files
    }

    pub fn find_node(&self, path: &str) -> Option<&FileNode> {
        if self.full_path == path {
            return Some(self);
        }

        for child in &self.children {
            if let Some(node) = child.find_node(path) {
                return Some(node);
            }
        }

        None
    }

    pub fn recalculate_sizes(&mut self) -> (i64, usize) {
        if self.is_file {
            return (self.size, 1);
        }

        let mut total_size = 0;
        let mut total_files = 0;

        for child in &mut self.children {
            let (child_size, child_count) = child.recalculate_sizes();
            total_size += child_size;
            total_files += child_count;
        }

        self.size = total_size;
        self.file_count = total_files;
        (total_size, total_files)
    }
}

pub struct FileTree {
    pub root: FileNode,
}

impl FileTree {
    pub fn from_assets(assets: Vec<SophonManifestAssetProperty>) -> Self {
        let mut root = FileNode::new_folder("root".to_string(), "".to_string());

        for asset in assets {
            Self::add_asset_to_tree(&mut root, asset);
        }

        root.sort_children();
        root.recalculate_sizes();

        Self { root }
    }

    fn add_asset_to_tree(root: &mut FileNode, asset: SophonManifestAssetProperty) {
        let path = asset.asset_name.clone();
        let parts: Vec<&str> = path.split('/').collect();

        let mut current = root;
        let mut current_path = String::new();

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;

            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            if is_last {
                // This is the file
                let file_node = FileNode::new_file(
                    part.to_string(),
                    current_path.clone(),
                    asset.asset_size,
                    asset.clone(),
                );
                current.add_child(file_node);
            } else {
                // This is a folder
                let child_exists = current.children.iter().position(|c| c.name == *part);

                if let Some(idx) = child_exists {
                    current = &mut current.children[idx];
                } else {
                    let folder_node = FileNode::new_folder(part.to_string(), current_path.clone());
                    current.children.push(folder_node);
                    let idx = current.children.len() - 1;
                    current = &mut current.children[idx];
                }
            }
        }
    }

    pub fn print_tree(&self, max_depth: Option<usize>) {
        self.print_node(&self.root, 0, "", max_depth);
    }

    fn print_node(&self, node: &FileNode, depth: usize, prefix: &str, max_depth: Option<usize>) {
        if let Some(max) = max_depth {
            if depth > max {
                return;
            }
        }

        if depth > 0 {
            let icon = if node.is_file { "📄" } else { "📁" };
            let size_str = crate::utils::format_size(node.size as u64);
            let count_str = if !node.is_file && node.file_count > 0 {
                format!(" ({} files)", node.file_count)
            } else {
                String::new()
            };

            println!(
                "{}{} {} - {}{}",
                prefix, icon, node.name, size_str, count_str
            );
        }

        if !node.is_file {
            let new_prefix = if depth == 0 {
                String::new()
            } else {
                format!("{}  ", prefix)
            };
            for child in &node.children {
                self.print_node(child, depth + 1, &new_prefix, max_depth);
            }
        }
    }

    pub fn get_folder_list(&self, depth: usize) -> Vec<(String, String, i64, usize)> {
        let mut folders = Vec::new();
        self.collect_folders(&self.root, 0, depth, &mut folders);
        folders
    }

    fn collect_folders(
        &self,
        node: &FileNode,
        current_depth: usize,
        max_depth: usize,
        folders: &mut Vec<(String, String, i64, usize)>,
    ) {
        if current_depth > max_depth {
            return;
        }

        if !node.is_file && current_depth > 0 {
            folders.push((
                node.name.clone(),
                node.full_path.clone(),
                node.size,
                node.file_count,
            ));
        }

        for child in &node.children {
            if !child.is_file {
                self.collect_folders(child, current_depth + 1, max_depth, folders);
            }
        }
    }
}
