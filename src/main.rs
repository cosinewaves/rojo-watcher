use notify::{ RecommendedWatcher, RecursiveMode, Result, Watcher, EventKind };
use std::sync::mpsc::channel;
use std::fs;
use std::path::Path;
use serde_json::{ Value, json };
use dialoguer;

fn main() -> Result<()> {
    // pick the Rojo project JSON file
    let json_file = rfd::FileDialog
        ::new()
        .add_filter("Rojo Project", &["json"])
        .set_title("Select Rojo .project.json file")
        .pick_file();

    let Some(json_path) = json_file else {
        println!("(file-watcher) no json file selected, exiting");
        return Ok(());
    };

    println!("(file-watcher) selected Rojo file: {:?}", json_path);

    // pick the folder to watch
    let folder = rfd::FileDialog::new().set_title("Select a folder to watch").pick_folder();

    let Some(path) = folder else {
        println!("(file-watcher) no folder selected, exiting");
        return Ok(());
    };

    println!("(file-watcher) watching for new folders in: {:?}", path);

    let (tx, rx) = channel();

    let mut watcher: RecommendedWatcher = Watcher::new(tx, notify::Config::default())?;
    watcher.watch(&path, RecursiveMode::NonRecursive)?;

    for res in rx {
        match res {
            Ok(event) => {
                if let EventKind::Create(_) = event.kind {
                    for new_folder_path in event.paths {
                        if new_folder_path.is_dir() {
                            println!("(file-watcher) new folder created: {:?}", new_folder_path);

                            if let Err(e) = handle_new_folder(&json_path, &path, &new_folder_path) {
                                eprintln!("(file-watcher) failed to update JSON: {:?}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => println!("(file-watcher) watch error: {:?}", e),
        }
    }

    Ok(())
}

/// handles inserting a new folder entry into the Rojo JSON file.
fn handle_new_folder(
    json_path: &Path,
    _watched_root: &Path,
    new_folder: &Path
) -> std::io::Result<()> {
    // read and parse the current JSON file
    let contents = fs::read_to_string(json_path)?;
    let mut data: Value = serde_json::from_str(&contents)?;

    // create the relative path (relative to JSON file so that stuff like src/folder works instead of Users/.../.../)
    let relative_path = pathdiff
        ::diff_paths(new_folder, json_path.parent().unwrap())
        .unwrap_or_else(|| new_folder.to_path_buf());

    let folder_name = new_folder.file_name().unwrap().to_string_lossy().to_string();

    // dynamically get possible parents from the JSON
    let mut possible_parents = Vec::new();
    if let Some(tree) = data.get("tree") {
        if let Some(tree_obj) = tree.as_object() {
            for key in tree_obj.keys() {
                possible_parents.push(key.clone());
            }
        }
    }

    // ensure we always have "Root (top-level)" as a fallback
    possible_parents.push("Root (top-level)".to_string());

    // prompt the user where to put this folder
    let selection = dialoguer::Select
        ::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(format!("Where should '{}' be added in the Rojo JSON?", folder_name))
        .default(0)
        .items(&possible_parents)
        .interact()
        .unwrap();

    let parent_choice = &possible_parents[selection];

    // insert new entry into the chosen section
    if let Some(tree) = data.get_mut("tree") {
        if let Some(tree_obj) = tree.as_object_mut() {
            if parent_choice == "Root (top-level)" {
                tree_obj.insert(folder_name.clone(), json!({ "$path": relative_path }));
                println!("(file-watcher) added '{}' at root", folder_name);
            } else if let Some(parent_obj) = tree_obj.get_mut(parent_choice) {
                if let Some(parent_map) = parent_obj.as_object_mut() {
                    parent_map.insert(folder_name.clone(), json!({ "$path": relative_path }));
                    println!("(file-watcher) added '{}' under '{}'", folder_name, parent_choice);
                } else {
                    println!("(file-watcher) '{}' exists but is not an object, skipping.", parent_choice);
                }
            } else {
                println!("(file-watcher) '{}' not found in JSON, adding at root instead.", parent_choice);
                tree_obj.insert(folder_name.clone(), json!({ "$path": relative_path }));
            }
        }
    } else {
        println!("(file-watcher) JSON file missing 'tree' key, skipping.");
        return Ok(());
    }

    // write back the updated JSON and format it
    let updated = serde_json::to_string_pretty(&data)?;
    fs::write(json_path, updated)?;

    println!("(file-watcher) JSON updated successfully.");

    Ok(())
}
