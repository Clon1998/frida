//! Reading and writing workspaces and collections on disk (PLAN.md §4).

mod collection;
mod paths;
mod workspace;

pub use collection::{
    TreeNode, create_collection, create_folder, create_request, delete_item, duplicate_request,
    load_collection, load_folder_chain, move_item, read_folder, read_request, rename_item, reorder,
    save_collection, save_folder, save_request, scan_tree,
};
pub use paths::{COLLECTION_FILE, FOLDER_FILE, WORKSPACE_FILE, slugify};
pub use workspace::{
    add_collection, collection_dir, create_collection_in_workspace, create_workspace,
    open_workspace, remove_collection, save_workspace,
};
