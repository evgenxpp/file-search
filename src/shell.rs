use std::{
    fs,
    io::{self, BufRead},
    path::PathBuf,
};

use tantivy::TantivyError;

use crate::search::{FileSearch, FileSearchWriteTransaction};

pub struct Shell {
    searcher: FileSearch,
}

impl Shell {
    pub fn new(searcher: FileSearch) -> Self {
        Self { searcher }
    }

    pub fn watch(&mut self) {
        self.handle_help_command();

        for line in io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    let mut parts = line.splitn(2, ' ');

                    if let Some(command) = parts.next() {
                        let arg = parts.next();

                        if !self.handle_command(command, arg) {
                            break;
                        }
                    }
                }
                Err(error) => eprintln!("{error}"),
            }
        }
    }

    fn handle_command(&mut self, name: &str, arg: Option<&str>) -> bool {
        let (name, arg) = (name.trim(), arg.map(str::trim));

        match (name, arg) {
            ("help", None) => self.handle_help_command(),
            ("clear", None) => self.handle_clear_command(),
            ("exit", None) => {
                return false;
            }
            ("add", Some(path)) => self.handle_add_command(path),
            ("remove", Some(path)) => self.handle_remove_command(path),
            ("upsert", Some(path)) => self.handle_update_command(path),
            ("search", Some(query)) => self.handle_search_command(query),
            _ => eprintln!("Unknown command: {name} {}", arg.unwrap_or_default()),
        }

        true
    }

    fn handle_help_command(&mut self) {
        println!("Commands:");
        println!("  help             Show this help message");
        println!("  add <path>       Add a new document");
        println!("  remove <path>    Remove an existing document");
        println!("  upsert <path>    Add or replace a document");
        println!("  clear            Remove all documents from index");
        println!("  search <query>   Search documents");
        println!("  exit             Exit the program");
        println!();
    }

    fn handle_clear_command(&mut self) {
        self.with_writer(|writer| writer.clear());
    }

    fn handle_add_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            self.with_writer(|writer| writer.append(&path));
        }
    }

    fn handle_remove_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            self.with_writer(|writer| writer.remove(&path));
        }
    }

    fn handle_update_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            self.with_writer(|writer| writer.upsert(&path));
        }
    }

    fn handle_search_command(&mut self, query: &str) {
        match self
            .searcher
            .open_read()
            .and_then(|reader| reader.search(query, None))
        {
            Ok(entries) => match serde_json::to_string(&entries) {
                Ok(json) => println!("{json}"),
                Err(error) => eprintln!("Cannot serialize found entries: {error}"),
            },
            Err(error) => eprintln!("Failed to search documents: {error}"),
        }
    }

    fn with_writer<F>(&mut self, f: F)
    where
        F: FnOnce(&mut FileSearchWriteTransaction) -> Result<(), TantivyError>,
    {
        match self.searcher.open_write() {
            Ok(mut writer) => {
                if let Err(error) = f(&mut writer).and_then(|_| writer.commit()) {
                    eprintln!("{error}");
                }
            }
            Err(err) => eprintln!("Failed to open writer: {err}"),
        }
    }

    fn resolve_file_path(path: &str) -> Option<PathBuf> {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Some(PathBuf::from(path)),
            Ok(_) => {
                eprintln!("Path is not a file: {path}");
                None
            }
            Err(error) => {
                eprintln!("Cannot access file: {error}");
                None
            }
        }
    }
}
