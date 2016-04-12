use std::iter::Iterator;
use std::path::Path;
use regex::Regex;
use utils::modify_file;
use app_result::AppResult;

/// Sort `str_lines` by the vi tag type. This ensures that tags
/// for `struct` and `enum` are always in front of other tags
/// for the same name and therefore these are the first found tags.
pub fn sort_lines(str_lines: Vec<&str>) -> Vec<&str> {
    let mut lines: Vec<_> = str_lines.iter().map(|l | { Line::parse(l) }).collect();
    lines.sort();
    lines.iter().map(|l| { l.line }).collect()
}

/// Sort the lines of `file` by `sort_lines`.
pub fn sort_file(file: &Path) -> AppResult<()> {
    modify_file(file, |contents| {
        let mut lines: Vec<_> = contents.lines().collect();
        lines = sort_lines(lines);

        let mut new_contents = String::with_capacity(contents.len());
        for line in &lines {
            new_contents.push_str(*line);
            new_contents.push_str("\n");
        }

        new_contents
    })
}

/// Represents one line in the vi tags format.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Line<'a> {
    /// the kind of vi tag line
    pub kind: Kind<'a>,

    /// the complete vi tag line
    pub line: &'a str
}

/// The kind of vi tag line.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind<'a> {
    /// A header in the vi tags file e.g.:
    ///
    ///     !_TAG_FILE_SORTED	1	/0=unsorted, 1=sorted, 2=foldcase/ 
    ///
    Header,

    /// A tag line in the format '{tagname}<Tab>{tagfile}<Tab>{tagaddress}' e.g:
    ///
    ///     Config	/home/dan/projekte/rusty-tags/src/config.rs	/^impl Config {$/;"	i
    ///
    Tag {
        name: &'a str,
        address_type: AddressType
    },

    /// Anything else
    Other 
}

impl<'a> Line<'a> {
    pub fn parse(line: &str) -> Line {
        if line.is_empty() {
            return Line { kind: Kind::Other, line: line };
        }

        if let Some('!') = line.chars().nth(0) {
            return Line { kind: Kind::Header, line: line };
        }

        let split = line.split('\t').collect::<Vec<&str>>();
        if split.len() < 3 {
            println!("Expected at least three elements separated by tab in the tag line. But got: '{:?}' from '{}'!", split, line);
            return Line { kind: Kind::Other, line: line };
        }

        Line { kind: Kind::Tag { name: split[0], address_type: AddressType::parse(split[2]) }, line: line }
    }
}

/// Represents what the `tagaddress` of the tag line contains.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum AddressType {
    Struct = 0,
    Enum   = 1,
    Other  = 5
}

impl AddressType {
    pub fn parse(tag_address: &str) -> AddressType {
        lazy_static! {
            static ref STRUCT: Regex = Regex::new(r#"^/\^\s*(pub )?struct.*$"#).unwrap();
            static ref ENUM  : Regex = Regex::new(r#"^/\^\s*(pub )?enum.*$"#).unwrap();
        }

        if STRUCT.is_match(tag_address) {
            return AddressType::Struct;
        } else if ENUM.is_match(tag_address) {
            return AddressType::Enum;
        }

        AddressType::Other
    }
}

#[test]
fn address_type_test() {
    assert_eq!(AddressType::parse(r#"/^impl Exec"#), AddressType::Other);
    assert_eq!(AddressType::parse(r#"/^pub struct FindMatches<'r, 't>(FindMatchesInner<'r, 't>);$/;"	s"#), AddressType::Struct);
    assert_eq!(AddressType::parse(r#"/^   pub struct FindMatches<'r, 't>(FindMatchesInner<'r, 't>);$/;"	s"#), AddressType::Struct);
    assert_eq!(AddressType::parse(r#"/^struct FindMatches<'r, 't>(FindMatchesInner<'r, 't>);$/;"	s"#), AddressType::Struct);
    assert_eq!(AddressType::parse(r#"/^        struct FindMatches<'r, 't>(FindMatchesInner<'r, 't>);$/;"	s"#), AddressType::Struct);
    assert_eq!(AddressType::parse(r#"/^pub enum Error {$/;"	g"#), AddressType::Enum);
    assert_eq!(AddressType::parse(r#"/^    pub enum Error {$/;"	g"#), AddressType::Enum);
    assert_eq!(AddressType::parse(r#"/^enum Error {$/;"	g"#), AddressType::Enum);
    assert_eq!(AddressType::parse(r#"/^      enum Error {$/;"	g"#), AddressType::Enum);
}

#[test]
fn line_test() {
    let line = r#"!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;" to lines/"#;
    assert_eq!(Line::parse(line), Line { kind: Kind::Header, line: line });

    let line = r#"Bar	/home/dan/.cargo/registry/src/github.com-88ac128001ac3a9a/toml-0.1.28/src/encoder/rustc_serialize.rs	/^        struct Bar { a: isize }$/;"	s"#;
    assert_eq!(Line::parse(line), Line { kind: Kind::Tag { name: "Bar", address_type: AddressType::Struct }, line: line });

    let line = r#"Bar	/home/dan/.cargo/registry/src/github.com-88ac128001ac3a9a/toml-0.1.28/src/encoder/rustc_serialize.rs	/^pub struct Bar { a: isize }$/;"	s"#;
    assert_eq!(Line::parse(line), Line { kind: Kind::Tag { name: "Bar", address_type: AddressType::Struct }, line: line });

    let line = r#"AddressType	/home/dan/projekte/rusty-tags/src/vi_tag.rs	/^impl AddressType {$/;"	i"#;
    assert_eq!(Line::parse(line), Line { kind: Kind::Tag { name: "AddressType", address_type: AddressType::Other }, line: line });

    {
        let line1 = r#"Bar	/home/dan/.cargo/registry/src/github.com-88ac128001ac3a9a/toml-0.1.28/src/encoder/rustc_serialize.rs	/^        struct Bar { a: isize }$/;"	s"#;
        let line2 = r#"CCC"#;
        let line3 = r#"AddressType	/home/dan/projekte/rusty-tags/src/vi_tag.rs	/^impl AddressType {$/;"	i"#;
        let line4 = r#"!_TAG_FILE_FORMAT	2	/extended format; --format=1 will not append ;" to lines/"#;

        let str_lines = vec![line1, line2, line3, line4];
        let mut lines: Vec<_> = str_lines.iter().map(|l | { Line::parse(l) }).collect();
        lines.sort();

        assert_eq!(lines[0].line, line4);
        assert_eq!(lines[1].line, line3);
        assert_eq!(lines[2].line, line1);
        assert_eq!(lines[3].line, line2);
    }

    {
        let line1 = r#"Config	/home/dan/projekte/rusty-tags/src/config.rs	/^impl Config {$/;"	i"#;
        let line2 = r#"Config	/home/dan/projekte/rusty-tags/src/config.rs	/^pub struct Config {$/;"	s"#;

        let str_lines = vec![line1, line2];
        let mut lines: Vec<_> = str_lines.iter().map(|l | { Line::parse(l) }).collect();
        lines.sort();

        assert_eq!(lines[0].line, line2);
        assert_eq!(lines[1].line, line1);
    }
}
