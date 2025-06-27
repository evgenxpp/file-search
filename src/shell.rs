use std::{
    fs,
    io::{self, BufRead},
    path::PathBuf,
};

use tantivy::TantivyError;

use crate::search::{FileSearch, FileSearchWriteTransaction};

pub struct Shell {
    searcher: FileSearch,
    writer: Option<Result<FileSearchWriteTransaction, TantivyError>>,
    is_flushed: bool,
}

impl Shell {
    pub fn new(searcher: FileSearch) -> Self {
        Self {
            searcher,
            writer: None,
            is_flushed: true,
        }
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
            ("commit", None) => self.handle_commit_command(),
            ("rollback", None) => self.handle_rollback_command(),
            ("exit", None) => {
                if !self.is_flushed {
                    self.handle_rollback_command();
                }

                return false;
            }
            ("add", Some(path)) => self.handle_add_command(path),
            ("remove", Some(path)) => self.handle_remove_command(path),
            ("upsert", Some(path)) => self.handle_update_command(path),
            ("search", Some(query)) => self.handle_search_command(query),
            _ => {
                eprintln!("Unknown command: {name} {}", arg.unwrap_or_default());
                eprintln!("Type 'help' to see available commands.");
            }
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
        println!("  commit           Commit pending changes");
        println!("  rollback         Undo pending changes");
        println!("  search <query>   Search documents");
        println!("  exit             Exit the program");
        println!();
    }

    fn handle_clear_command(&mut self) {
        let result = self.with_writer(|writer| writer.clear());

        if result && self.is_flushed {
            self.is_flushed = false;
        }
    }

    fn handle_commit_command(&mut self) {
        if self.is_flushed {
            eprintln!("No changes to commit");
        } else {
            self.is_flushed = self.with_writer(|writer| writer.commit());
        }
    }

    fn handle_rollback_command(&mut self) {
        if self.is_flushed {
            eprintln!("No changes to rollback");
        } else {
            self.is_flushed = self.with_writer(|writer| writer.rollback());
        }
    }

    fn handle_add_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            let result = self.with_writer(|writer| writer.append(&path));

            if result && self.is_flushed {
                self.is_flushed = false;
            }
        }
    }

    fn handle_remove_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            let result = self.with_writer(|writer| writer.remove(&path));

            if result && self.is_flushed {
                self.is_flushed = false;
            }
        }
    }

    fn handle_update_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            let result = self.with_writer(|writer| writer.upsert(&path));

            if result && self.is_flushed {
                self.is_flushed = false;
            }
        }
    }

    fn handle_search_command(&mut self, query: &str) {
        if self.is_flushed {
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
        } else {
            eprintln!("You have uncommitted changes. Please commit or rollback before searching")
        }
    }

    fn with_writer<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&mut FileSearchWriteTransaction) -> Result<(), TantivyError>,
    {
        if self.writer.is_none() {
            self.writer = Some(self.searcher.open_write());
        }

        let writer = self.writer.as_mut().unwrap();

        match writer {
            Ok(writer) => {
                if let Err(error) = f(writer) {
                    eprintln!("Failed to change index: {error}");
                } else {
                    return true;
                }
            }
            Err(err) => eprintln!("Unable to start write session: {err}"),
        }

        false
    }

    fn resolve_file_path(path: &str) -> Option<PathBuf> {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Some(PathBuf::from(path)),
            Ok(_) => {
                eprintln!("The path '{path}' is not a file");
                None
            }
            Err(error) => {
                eprintln!("Failed to access file '{path}': {error}");
                None
            }
        }
    }
}
