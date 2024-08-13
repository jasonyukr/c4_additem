use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter};
use std::io::Write;
use indexmap::IndexSet;

const LIMIT: usize = 20000;
const DATA_FILENAME: &str = ".recent.txt";

fn parse_line(input: &str) -> Vec<String> {
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

fn add_parsed_arguments(loaded_data: &mut IndexSet<String>, datafile: &str, home: &str, pwd: &str, input: &str, new_data: &mut IndexSet<String>) -> u32 {
    let token_list = parse_line(input);

    let mut cnt = 0;
    let mut updated = 0;
    for arg in token_list {
        cnt += 1;
        if cnt == 1 {
            if !arg.starts_with("./") && !arg.starts_with("../") && !arg.starts_with("/") && !arg.starts_with("~/") {
                // ignore the command itself if the command doesn't start with "." "/" "~"
                continue;
            }
        }
        if arg.starts_with('-') {
            // ignore option parameters like "--color"
            continue;
        }
        if arg.starts_with("/dev/") {
            // ignore the device driver access
            continue;
        }

        // Compose the fullpath with the path expansion
        let mut fullarg: String;
        if arg.starts_with("~/") {
            fullarg = home.to_string();
            fullarg.push_str(&arg[1..]);
        } else if arg.starts_with('/') {
            fullarg = arg;
        } else {
            fullarg = pwd.to_string();
            fullarg.push_str("/");
            fullarg.push_str(&arg);
        }

        // Append the canonalized fullpath only if the file exists
        match fs::canonicalize(&fullarg) {
            Ok(path) => {
                if let Some(cano_str) = path.to_str() {
                    if cano_str.eq(datafile) {
                        // ignore the data file itself
                        continue;
                    }
                    // exists() is double-checking as canonalized() is basically for existing file/dir
                    if path.exists() { // && path.is_file() {
                        if loaded_data.contains(cano_str) {
                            loaded_data.shift_remove(cano_str);
                        }
                        new_data.insert(cano_str.to_string());
                        updated += 1;
                    }
                }
            },
            _ => { },
        }
    }
    updated
}

fn main() {
    let home = std::env::var("HOME").unwrap();
    let filename = format!("{}/{}", home, DATA_FILENAME);

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let pwd = lines.next().unwrap_or(Ok(String::new())).unwrap();
    let cmd = lines.next().unwrap_or(Ok(String::new())).unwrap();
    if pwd.len() == 0 || cmd.len() == 0 {
        return;
    }

    // Load the data file
    let mut new_data = IndexSet::new();
    let mut loaded_data = IndexSet::new();
    let file = File::open(&filename);
    match file {
        Ok(file) => {
            let reader = BufReader::new(file);
            for (_, line) in reader.lines().enumerate() {
                let line = line.unwrap();
                loaded_data.insert(line);
            }
        },
        _ => { },
    };

    if add_parsed_arguments(&mut loaded_data, &filename, &home, &pwd, &cmd, &mut new_data) == 0 || new_data.len() == 0 {
        return;
    }

    // Save the updated data file (new_data + loaded_data) and keep the LIMIT
    if let Ok(file) = File::create(&filename) {
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
    }
}
