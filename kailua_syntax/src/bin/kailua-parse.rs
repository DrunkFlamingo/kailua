extern crate kailua_syntax;

use std::env;
use std::fs;
use std::io::Read;

fn parse_and_dump(path: &str) -> Result<(), String> {
    let mut f = try!(fs::File::open(path).map_err(|e| e.to_string()));
    let mut buf = Vec::new();
    try!(f.read_to_end(&mut buf).map_err(|e| e.to_string()));
    drop(f);

    // strip any BOM
    let mut offset = 0;
    if &buf[0..3] == b"\xef\xbb\xbf" {
        offset = 3;
    }

    let chunk = try!(kailua_syntax::parse_chunk(&buf[offset..]).map_err(|e| e.to_string()));
    println!("{:?}", chunk);
    Ok(())
}

pub fn main() {
    for path in env::args().skip(1) {
        println!("--== {} ==--", path);
        if let Err(e) = parse_and_dump(&path) {
            println!("error: {}", e);
        }
        println!("");
    }
}
