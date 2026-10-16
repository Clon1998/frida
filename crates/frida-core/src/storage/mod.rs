//! Reading and writing workspaces and collections on disk (PLAN.md §4).

mod collection;
mod environments;
pub(crate) mod paths;
mod workspace;

pub use collection::{
    TreeNode, create_collection, create_folder, create_request, delete_item, duplicate_request,
    load_collection, load_folder_chain, move_item, read_folder, read_request, rename_item, reorder,
    save_collection, save_folder, save_request, scan_tree,
};
pub use environments::{
    add_environment, create_environment, delete_environment, duplicate_environment,
    environment_from_yaml, environment_to_yaml, find_environment, free_environment_name,
    list_environments, reorder_environments, save_environment,
};
pub use paths::{COLLECTION_FILE, FOLDER_FILE, WORKSPACE_FILE, slugify};
pub use workspace::{
    COLLECTIONS_DIR, CollectionSummary, WorkspaceOverview, add_collection, collection_dir,
    create_collection_in_workspace, create_named_collection, create_workspace, load_overview,
    new_collection_dir, open_workspace, remove_collection, save_workspace,
};
