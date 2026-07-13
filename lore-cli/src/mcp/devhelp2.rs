use quick_xml::de::from_reader;
use serde::Deserialize;
use std::io::BufReader;

#[derive(Debug, Deserialize)]
pub struct Chapter {
    #[serde(rename = "@name")]
    pub name: String,

    #[serde(rename = "@link")]
    pub link: String,

    #[serde(rename = "sub", default)]
    pub children: Vec<Chapter>,
}

#[derive(Debug, Deserialize)]
pub struct Keyword {
    #[serde(rename = "@type")]
    pub kind: String,

    #[serde(rename = "@name")]
    pub name: String,

    #[serde(rename = "@link")]
    pub link: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "book")]
pub struct Book {
    #[serde(rename = "@title")]
    pub title: String,

    #[serde(rename = "@name")]
    pub name: String,

    #[serde(rename = "@link")]
    pub link: String,

    #[serde(rename = "@version")]
    pub version: String,

    #[serde(rename = "@language")]
    pub language: String,

    #[serde(rename = "chapters")]
    chapters: ChaptersWrapper,

    #[serde(rename = "functions")]
    functions: FunctionsWrapper,
}

impl Book {
    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters.items
    }

    pub fn keywords(&self) -> &[Keyword] {
        &self.functions.items
    }
}

#[derive(Debug, Deserialize)]
struct ChaptersWrapper {
    #[serde(rename = "sub", default)]
    items: Vec<Chapter>,
}

#[derive(Debug, Deserialize)]
struct FunctionsWrapper {
    #[serde(rename = "keyword", default)]
    items: Vec<Keyword>,
}

/// Parse devhelp2 XML from bytes (already decompressed by lore-core if needed)
pub fn parse_devhelp(data: &[u8]) -> Result<Book, Box<dyn std::error::Error>> {
    let reader = BufReader::new(data);
    Ok(from_reader(reader)?)
}
