use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::ToolError;

pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<Vec<String>, ToolError> {
    if !path.exists() {
        return Err(ToolError::Execution(format!(
            "log file does not exist: {}",
            path.display()
        )));
    }

    let bounded_max = max_lines.clamp(1, 1000);
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut tail = VecDeque::with_capacity(bounded_max);

    for line in reader.lines() {
        let line = line?;
        if tail.len() == bounded_max {
            let _ = tail.pop_front();
        }
        tail.push_back(line);
    }

    Ok(tail.into_iter().collect())
}

pub fn sha256_file(path: &Path) -> Result<String, ToolError> {
    if !path.exists() {
        return Err(ToolError::Execution(format!(
            "file does not exist: {}",
            path.display()
        )));
    }

    let mut file = File::open(path)?;
    let mut buffer = [0u8; 8192];
    let mut hasher = Sha256::new();

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}
