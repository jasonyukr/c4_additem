use fs2::FileExt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter};
use std::io::Write;
use std::path::Path;
use indexmap::IndexSet;

const LIMIT: usize = 20000;
const DATA_FILENAME: &str = ".recent.txt";

#[derive(PartialEq)]
enum Command {
    Cd,
    Cp,
    Mv,
    Scp,
    Ssh,
    Rm,
    Rmdir,
    Etc
}

fn get_line_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape = false;

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if escape {
            if c == ' ' {
                current_token.push(' ');
            } else if c == '\'' {
                current_token.push('\'');
            } else if c == '"' {
                current_token.push('"');
            } else {
                current_token.push(c);
            }
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == '"' {
            in_double_quote = !in_double_quote;
            if !in_double_quote && !in_single_quote {
                tokens.push(current_token.trim().to_string());
                current_token.clear();
            }
        } else if c == '\'' {
            in_single_quote = !in_single_quote;
            if !in_single_quote && !in_double_quote {
                tokens.push(current_token.trim().to_string());
                current_token.clear();
            }
        } else if c == ' ' && !in_single_quote && !in_double_quote {
            if !current_token.is_empty() {
                tokens.push(current_token.trim().to_string());
                current_token.clear();
            }
        } else {
            current_token.push(c);
        }
    }

    if !current_token.trim().is_empty() {
        tokens.push(current_token.trim().to_string());
    }

    tokens
}

fn get_filesystem_object_list(input: &str, home: &str, pwd: &str, list: &mut Vec<String>) -> Command {
    let tokens = get_line_tokens(input);

    let mut cmd: Command = Command::Etc;
    let mut cnt = 0;
    for token in tokens {
        cnt += 1;
        if cnt == 1 {
            if token.eq("sudo") {
                // ignore sudo token and continue
                cnt = 0;
                continue;
            }
            if token.eq("cd") {
                cmd = Command::Cd;
            } else if token.eq("cp") {
                cmd = Command::Cp;
            } else if token.eq("mv") {
                cmd = Command::Mv;
            } else if token.eq("scp") {
                cmd = Command::Scp;
            } else if token.eq("ssh") {
                cmd = Command::Ssh;
            } else if token.eq("rm") {
                cmd = Command::Rm;
            } else if token.eq("rmdir") {
                cmd = Command::Rmdir;
            }
            if !token.starts_with("./") && !token.starts_with("../") && !token.starts_with("/") && !token.starts_with("~/") {
                // ignore the command itself if the command doesn't start with "." "/" "~"
                continue;
            }
        }
        if token.starts_with('-') {
            if cmd.eq(&Command::Cp) || cmd.eq(&Command::Mv) {
                if token.eq("-t") || token.starts_with("--target-directory") {
                    // "-t, --target-directory=DIRECTORY" is special case that swaps the argument order. Ignore this case.
                    list.clear();
                    return cmd;
                }
            }
            // ignore option parameters like "--color"
            continue;
        }
        if token.starts_with("/dev/null") {
            // ignore the null device driver access
            continue;
        }

        // Compose the fullpath with the path expansion
        let mut fullpath: String;
        if token.starts_with("~/") {
            fullpath = home.to_string();
            fullpath.push_str(&token[1..]);
        } else if token.starts_with('/') {
            fullpath = token;
        } else if let Some(_idx) = token.find(":") {
            // Special case URI for scp
            //   [user@]host:[path]  or
            //   scp://[user@]host[:port][/path]
            fullpath = token;
        } else if cmd == Command::Ssh {
            fullpath = token;
        } else {
            fullpath = pwd.to_string();
            fullpath.push_str("/");
            fullpath.push_str(&token);
        }

        list.push(fullpath);
    }
    cmd
}

fn handle_filesystem_object(cmd: &Command, fs_object: &str, data_filename: &str, loaded_data: &mut IndexSet<String>, new_data: &mut IndexSet<String>) {

    if cmd.eq(&Command::Scp) {
        if let Some(_idx) = fs_object.find(":") {
            // special case URI for scp (e.g. id@server:/root/here)
            let scp_entry = format!("SCP#{}", fs_object);
            if loaded_data.contains(&scp_entry) {
                if let Some(first) = loaded_data.iter().next() {
                    if first == &scp_entry {
                        // No reason to update the data file if the first item (most recent item)
                        // is already the same with new item.
                        return;
                    }
                }
                loaded_data.shift_remove(&scp_entry);
            }
            new_data.insert(scp_entry.to_string());
            return;
        }
        // local path parameter for scp should fall-through
    }

    if cmd.eq(&Command::Ssh) {
        let ssh_entry = format!("SSH#{}", fs_object);
        if loaded_data.contains(&ssh_entry) {
            if let Some(first) = loaded_data.iter().next() {
                if first == &ssh_entry {
                    // No reason to update the data file if the first item (most recent item)
                    // is already the same with new item.
                    return;
                }
            }
            loaded_data.shift_remove(&ssh_entry);
        }
        new_data.insert(ssh_entry.to_string());
        return;
    }

    match fs::canonicalize(&fs_object) {
        Ok(path) => {
            if let Some(cano_str) = path.to_str() {
                // exists() is double-checking as canonalized() is basically for existing file/dir
                if path.exists() {
                    if path.is_file() {
                        if cano_str.eq(data_filename) {
                            // ignore the data file itself
                            return;
                        }
                        if loaded_data.contains(cano_str) {
                            if let Some(first) = loaded_data.iter().next() {
                                if first == cano_str {
                                    // No reason to update the data file if the first item (most recent item)
                                    // is already the same with new item.
                                    return;
                                }
                            }
                            loaded_data.shift_remove(cano_str);
                        }
                        new_data.insert(cano_str.to_string());
                    } else if path.as_path().is_dir() {
                        let mut dir_cano_str = format!("{}/", cano_str);
                        if cano_str.eq("/") {
                            dir_cano_str = format!("/");
                        }
                        if cmd.eq(&Command::Cd) {
                            dir_cano_str.push(' '); // append space to mark the "cd" result
                        }
                        if loaded_data.contains(&dir_cano_str) {
                            if let Some(first) = loaded_data.iter().next() {
                                if first == &dir_cano_str {
                                    // No reason to update the data file if the first item (most recent item)
                                    // is already the same with new item.
                                    return;
                                }
                            }
                            loaded_data.shift_remove(&dir_cano_str);
                        }
                        new_data.insert(dir_cano_str.to_string());
                    }
                }
            }
        },
        _ => { },
    }
}

fn update_data(cmd: &Command, objects: &mut Vec<String>, data_filename: &str, loaded_data: &mut IndexSet<String>, new_data: &mut IndexSet<String>) {
    let mut cmd_target_dir: String = "".to_string();
    if (cmd.eq(&Command::Cp) || cmd.eq(&Command::Mv)) && objects.len() >= 2 {
        if let Some(last) = objects.last() {
            match fs::canonicalize(&last) {
                Ok(path) => {
                    if let Some(cano_str) = path.to_str() {
                        if path.as_path().is_dir() {
                            // If the last entry is directory for cp/mv, move the dirname string to cmd_target_dir for later processing
                            cmd_target_dir = cano_str.to_string();
                            objects.pop();
                        }
                    }
                },
                _ => {},
            }
        }
    }

    if cmd.eq(&Command::Ssh) && objects.len() != 1 {
        // For simplicity, we just accept the one argument case
        return;
    }

    for obj in objects {
        // handle filesystem object in the list directly
        handle_filesystem_object(cmd, obj, data_filename, loaded_data, new_data);

        if !cmd_target_dir.is_empty() {
            if let Some(leaf) = Path::new(obj).file_name() {
                if let Some(leaf_str) = leaf.to_str() {
                    // handle composed new pathname for cp/mv with last dir argument
                    let composed_obj = format!("{}/{}", cmd_target_dir, leaf_str);
                    handle_filesystem_object(cmd, &composed_obj, data_filename, loaded_data, new_data);
                }
            }
        }
    }
}

fn main() {
    let home = std::env::var("HOME").unwrap();
    let data_filename = format!("{}/{}", home, DATA_FILENAME);

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let pwd = lines.next().unwrap_or(Ok(String::new())).unwrap();
    let input = lines.next().unwrap_or(Ok(String::new())).unwrap();
    if pwd.len() == 0 || input.len() == 0 {
        return;
    }

    // Parse argument first
    let mut fs_objects = Vec::new();
    let cmd = get_filesystem_object_list(&input, &home, &pwd, &mut fs_objects);
    if fs_objects.len() == 0 {
        return;
    }

    // Load the data file
    let mut new_data = IndexSet::new();
    let mut loaded_data = IndexSet::new();
    let file = File::open(&data_filename);
    match file {
        Ok(file) => {
            let _ = file.lock_exclusive(); // locks the file, blocking if the file is currently locked
            let reader = BufReader::new(&file);
            for (_, line) in reader.lines().enumerate() {
                let line = line.unwrap();
                loaded_data.insert(line);
            }
            let _ = file.unlock(); // unlock the file
        },
        _ => { },
    };

    update_data(&cmd, &mut fs_objects, &data_filename, &mut loaded_data, &mut new_data);
    if new_data.len() == 0 {
        return;
    }

    // Save the updated data file (new_data + loaded_data) and keep the LIMIT
    if let Ok(file) = File::create(&data_filename) {
        let _ = file.lock_exclusive(); // locks the file, blocking if the file is currently locked
        let mut writer = BufWriter::new(&file);
        let new_data_len = new_data.len();
        for arg in new_data {
            let _ = writer.write(arg.as_bytes());
            let _ = writer.write("\n".as_bytes());
        }
        if new_data_len < LIMIT {
            let mut loaded_data_len = loaded_data.len();
            if new_data_len + loaded_data_len > LIMIT {
                loaded_data_len = LIMIT - new_data_len;
            }
            for (i, arg) in loaded_data.iter().enumerate() {
                if i >= loaded_data_len {
                    break;
                }
                let _ = writer.write(arg.as_bytes());
                let _ = writer.write("\n".as_bytes());
            }
        }
        let _ = writer.flush();
        let _ = file.unlock(); // unlock the file
    }
}
