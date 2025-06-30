use std::{
    fs,
    io::{self, BufRead},
};

use crate::{
    error::Error,
    search::{FileSearch, FileSearchWriteTransaction},
};

pub struct Shell {
    searcher: FileSearch,
    writer: Option<FileSearchWriteTransaction>,
}

impl Shell {
    pub fn new(searcher: FileSearch) -> Self {
        Self {
            searcher,
            writer: None,
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
            ("list", None) => self.handle_list_command(),
            ("commit", None) => self.handle_commit_command(),
            ("rollback", None) => self.handle_rollback_command(),
            ("exit", None) => {
                self.handle_rollback_command();
                return false;
            }
            ("add", Some(path)) => self.handle_add_command(path),
            ("remove", Some(path)) => self.handle_remove_command(path),
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
        println!("  list             Show all documents");
        println!("  add <path>       Add a new document");
        println!("  remove <path>    Remove an existing document");
        println!("  clear            Remove all documents from index");
        println!("  commit           Commit pending changes");
        println!("  rollback         Undo pending changes");
        println!("  search <query>   Search documents");
        println!("  exit             Exit the program");
        println!();
    }

    fn handle_clear_command(&mut self) {
        self.with_writer(|writer| writer.clear());
    }

    fn handle_list_command(&mut self) {
        match self.searcher.open_read() {
            Ok(trx) => match trx.list() {
                Ok(entries) => match serde_json::to_string(&entries) {
                    Ok(json) => println!("{json}"),
                    Err(error) => eprintln!("Cannot serialize found entries. {error}"),
                },
                Err(error) => eprintln!("Cannot retrive documents. {error}"),
            },
            Err(err) => eprintln!("Unable to start read session. {err}"),
        }
    }

    fn handle_commit_command(&mut self) {
        match self.writer.take() {
            Some(writer) => {
                if let Err(error) = writer.commit() {
                    eprintln!("Failed to commit. {error}");
                }
            }
            _ => eprintln!("No changes to commit."),
        }
    }

    fn handle_rollback_command(&mut self) {
        match self.writer.take() {
            Some(writer) => {
                if let Err(error) = writer.rollback() {
                    eprintln!("Failed to rollback. {error}");
                }
            }
            _ => eprintln!("No changes to rollback."),
        }
    }

    fn handle_add_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            self.with_writer(|writer| writer.add(path));
        }
    }

    fn handle_remove_command(&mut self, path: &str) {
        if let Some(path) = Self::resolve_file_path(path) {
            self.with_writer(|writer| writer.remove(path));
        }
    }

    fn handle_search_command(&mut self, query: &str) {
        if self.writer.is_none() {
            match self
                .searcher
                .open_read()
                .and_then(|reader| reader.search(query, None))
            {
                Ok(entries) => match serde_json::to_string(&entries) {
                    Ok(json) => println!("{json}"),
                    Err(error) => eprintln!("Cannot serialize found entries. {error}"),
                },
                Err(error) => eprintln!("Failed to search documents. {error}"),
            }
        } else {
            eprintln!("You have uncommitted changes. Please commit or rollback before searching.")
        }
    }

    fn get_or_create_writer(&mut self) -> Result<&mut FileSearchWriteTransaction, Error> {
        match self.writer {
            Some(ref mut writer) => Ok(writer),
            _ => {
                let new_writer = self.searcher.open_write()?;
                self.writer = Some(new_writer);
                Ok(self.writer.as_mut().unwrap())
            }
        }
    }

    fn with_writer<F>(&mut self, f: F)
    where
        F: FnOnce(&mut FileSearchWriteTransaction) -> Result<(), Error>,
    {
        match self.get_or_create_writer() {
            Ok(writer) => {
                if let Err(error) = f(writer) {
                    eprintln!("Failed to change index. {error}");
                }
            }
            Err(err) => eprintln!("Unable to start write session. {err}"),
        }
    }

    fn resolve_file_path(path: &str) -> Option<&str> {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Some(path),
            Ok(_) => {
                eprintln!("The path '{path}' is not a file.");
                None
            }
            Err(error) => {
                eprintln!("Failed to access file '{path}'. {error}");
                None
            }
        }
    }
}
