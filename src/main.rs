use mdict_parser::{mdict::Mdx, parser::{self, KeyEntry}};
use clap::{error::Result, Parser};
use tklog::{debug, info, warn, LOG, Format, LEVEL};
use std::{error::Error, fs::File, io::{self, BufReader, Read}, sync::{mpsc::Receiver, Arc}};
use std::path::{Path, PathBuf};
use rusqlite::{Connection, Statement};
use regex::Regex;
use lazy_static::lazy_static;
use std::sync::mpsc;
use std::thread;
use num_cpus;

#[derive(Parser)]
#[command(version, author, about, long_about)]
struct Cli {
    /// The path to the mdx file
    mdx_path: String,

    /// The path to the output file(.db), defaults to <mdx_path>.db
    output_path: Option<String>,

    /// The number of threads to use, defaults to your cpu cores
    #[arg(short, long, default_value_t = num_cpus::get())]
    threads: usize,

    /// Remove <img> and <a> tags from the definition
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

fn find_definitions(send: mpsc::Sender<(String, String)>, mdx: Arc<Mdx>, keys: Vec<KeyEntry>){
    for key in keys {
        if let Err(e) = send.send((key.text.clone(), Mdx::find_definition(&mdx, &key))){
            println!("{}", e);
        }
    }
}

fn split_tasks(mdict: Arc<Mdx>, thread: usize) -> (Vec<Vec<KeyEntry>>, usize) {
    let mut tasks: Vec<Vec<KeyEntry>> = Vec::new();
    let mut n: usize = 0;
    for _ in 0..thread{
        tasks.push(Vec::new());
    }
    for (i, key) in mdict.keys().enumerate() {
        tasks[i % thread].push(key.clone());
        n += 1;
    }
    (tasks, n)
}

fn start_tasks(tasks: Vec<Vec<KeyEntry>>, mdx_dict: Arc<Mdx>) -> Receiver<(String, String)> {
    let (tx, rx): (mpsc::Sender<(String, String)>, mpsc::Receiver<(String, String)>) = mpsc::channel();
    for t in tasks{
        let tx = tx.clone();
        let _dict = mdx_dict.clone();
        thread::spawn(move ||{
            find_definitions(tx, _dict, t)
        });
    }
    rx
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
    let mdx_dict = Arc::new(mdx_dict);

    info!("Creating database...");
    let mut db = create_db(output_path)?;

    info!("Splitting tasks...");
    let (tasks, task_num) = split_tasks(mdx_dict.clone(), cli.threads);

    info!("Tasks starting...");
    let rx = start_tasks(tasks, mdx_dict);

    info!("Inserting data into database...");
    let trans = db.transaction()?;
    let start_time = std::time::Instant::now();
    let mut ins_cmd = trans.prepare("INSERT INTO stardict (word, source_html) values (?1, ?2)").unwrap();
    let mut inserted_num:usize = 0;
    loop{
        match rx.recv() {
            Ok((word, definition)) => {
                insert_to_db(&mut ins_cmd, &word, &definition, cli.remove_img_a)?;
                inserted_num += 1;
                if inserted_num % 6000 == 0 {
                    info!(format!("{} / {} inserted; {}%; speed:{}/s", inserted_num, task_num, inserted_num as f32 / task_num as f32 * 100.0, inserted_num as f32 / start_time.elapsed().as_secs_f32()));
                }
            }
            Err(e) => {
                println!("{}", e);
                break;
            }
        }
    }
    drop(ins_cmd);
    trans.commit()?;

    info!("Done in", start_time.elapsed().as_secs_f32());

    Ok(())
}
