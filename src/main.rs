use mdict_parser::parser;
use clap::{error::Result, Parser};
use tklog::{debug, info, warn, LOG, Format, LEVEL};
use std::{error::Error, fs::File, io::{self, BufReader, Read}};
use std::path::{Path, PathBuf};
use rusqlite::{Connection, Statement};
use regex::Regex;
use lazy_static::lazy_static;

#[derive(Parser)]
#[command(version, author, about, long_about)]
struct Cli {
    /// The path to the mdx file
    mdx_path: String,

    /// The path to the output file(.db), defaults to <mdx_path>.db
    output_path: Option<String>,

    #[arg(short, long)]
    remove_img_a: bool,
}

lazy_static! {
    static ref CLEAN_HTML:Regex = Regex::new(r"<img\b[^>]*>|</img>|<a\b[^>]*>|</a>").unwrap();
}

fn read_file(path: &str) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut buf = Vec::new();
    let mut reader = BufReader::new(file);
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

fn create_db(path: PathBuf) -> Result<Connection, Box<dyn Error + 'static>>{
    let init = !path.exists();
    let db = Connection::open(path)?;
    if init{
        debug!("Initializing database...");
        db.execute("CREATE TABLE stardict (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            word VARCHAR(64) NOT NULL,
            source_html TEXT NOT NULL)", ())?;
        db.execute("PRAGMA synchronous = OFF", ())?;

    }else{
        warn!("Database already exists, skipping initialization... If the database is not caeated by this tool, may cause some problems.");
    }
    Ok(db)
}
fn insert_to_db(ins_cmd: &mut Statement, word: &str, source_html: &str, clean: bool) -> Result<(), rusqlite::Error> {
    let mut processed_html = source_html.replace("'", "\"").replace("\"", r#"\""#);
    if clean {
        processed_html = (*CLEAN_HTML).replace_all(processed_html.trim(), "").to_string(); // Remove <img> <a> tags
    }

    let res = ins_cmd.execute((word, &processed_html));
    if let Err(e) = res {
        debug!("Err data:", processed_html);
        return Err(e);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error + 'static>> {
    LOG.set_level(LEVEL::Debug);
    LOG.set_format(Format::LevelFlag | Format::Time | Format::ShortFileName);

    let cli = Cli::parse();
    let mdx_path = Path::new(&cli.mdx_path);
    let output_path = match cli.output_path {
        Some(p) => PathBuf::from(p),
        None => mdx_path.with_extension("db"),
    };


    info!("Loading mdx file...");
    let input = read_file(mdx_path.to_str().unwrap())?;
    info!("Parsing mdx file...");
    let mdx_dict = parser::parse(&input);

    info!("Creating database...");
    let mut db = create_db(output_path)?;

    info!("Inserting data into database...");
    let trans = db.transaction()?;
    let start_time = std::time::Instant::now();
    let mut ins_cmd = trans.prepare("INSERT INTO stardict (word, source_html) values (?1, ?2)").unwrap();
    for (i, record) in mdx_dict.items().enumerate() {
        let res = insert_to_db(&mut ins_cmd, record.key, record.definition.as_str(), cli.remove_img_a);
        if let Err(e) = res {
            warn!(format!("Error inserting record {}: {}. Err: {}. Skipped", i, record.key, e));
        }
        if i % 3000 == 0{
            info!(format!("Inserted {} records... Speed: {} records/s", i, i as f32 / start_time.elapsed().as_secs_f32()));
        }
    }
    drop(ins_cmd);
    trans.commit()?;

    Ok(())
}
