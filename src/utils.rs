use std::fs::OpenOptions;
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use app_result::AppResult;

/// Reads `file` into a string which is passed to the function `f`
/// and the returned string of `f` is written back into `file`.
pub fn modify_file<F>(file: &Path, f: F) -> AppResult<()>
    where F: FnOnce(String) -> String
{
    let mut file = try!(OpenOptions::new()
        .read(true)
        .write(true)
        .open(file));

    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));

    let contents = f(contents);

    try!(file.set_len(contents.as_bytes().len() as u64));
    try!(file.seek(SeekFrom::Start(0)));
    try!(file.write_all(contents.as_bytes()));
    Ok(())
}
