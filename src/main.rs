use normalize_path::NormalizePath;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter};
use std::io::Write;
use std::path::Path;
use indexmap::IndexSet;
use regex::Regex;

const LIMIT: usize = 1000;
const DATA_FILENAME: &str = ".recent.txt";

fn add_parsed_arguments(data: &mut IndexSet<String>, home: &str, pwd: &str, input: &str) -> u32 {
    let re = Regex::new(r#""((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|(\S+)"#).unwrap();

    let mut cnt = 0;
    let mut updated = 0;
    for cap in re.captures_iter(input) {
        if let Some(m) = cap.get(1).or_else(|| cap.get(2)).or_else(|| cap.get(3)) {
            let arg = m.as_str().replace(r#"\ "#, " ");

            // Filter out the command itself and the option parameters like "--color"
            cnt += 1;
            if cnt == 1 {
                continue;
            }
            if arg.starts_with('-') {
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

            // Append the normalized fullpath only if the file exists
            let path = Path::new(&fullarg);
            let norm_path = path.normalize();
            if let Some(norm_str) = norm_path.to_str() {
                if norm_path.exists() && norm_path.is_file() {
                    if data.contains(norm_str) {
                        data.shift_remove(norm_str);
                    }
                    data.insert(norm_str.to_string());
                    updated += 1;
                }
            }
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
    let mut data = IndexSet::new();
    let file = File::open(&filename);
    match file {
        Ok(file) => {
            let reader = BufReader::new(file);
            for (_, line) in reader.lines().enumerate() {
                let line = line.unwrap();
                data.insert(line);
            }
        },
        _ => { },
    };

    if add_parsed_arguments(&mut data, &home, &pwd, &cmd) == 0 {
        return;
    }

    // Keep the max size of the IndexSet
    if data.len() > LIMIT {
        let diff = data.len() - LIMIT;
        data.drain(..diff);
    }

    // Save the updated data file
    if let Ok(file) = File::create(&filename) {
        let mut writer = BufWriter::new(&file);
        for arg in data {
            let _ = writer.write(arg.as_bytes());
            let _ = writer.write("\n".as_bytes());
        }
        let _ = writer.flush();
    }
}

