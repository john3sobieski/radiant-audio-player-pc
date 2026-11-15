//! Build-time tool (CI): glues the payload to the setup exe.
//!
//! Usage: `pack <stub.exe> <output.exe> <file> [<file> ...]`
//! Every `<file>` is stored under its own file name (`radiant.exe`, `onnxruntime.dll`, ...).

#[path = "payload.rs"]
mod payload;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: pack <stub.exe> <output.exe> <file> [<file> ...]");
        std::process::exit(2);
    }
    let stub = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    let mut files = Vec::new();
    for arg in &args[2..] {
        let path = PathBuf::from(arg);
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            eprintln!("bad file name: {arg}");
            std::process::exit(2);
        };
        if !path.is_file() {
            eprintln!("file not found: {arg}");
            std::process::exit(1);
        }
        files.push((name, path));
    }
    if let Err(e) = payload::write_packed(&stub, &files, &out) {
        eprintln!("pack failed: {e}");
        std::process::exit(1);
    }
    println!("packed {} files into {}", files.len(), out.display());
}
