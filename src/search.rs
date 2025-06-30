use std::{
    any::type_name,
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs,
    ops::Range,
    path::Path,
    time::UNIX_EPOCH,
};

use bincode::{Decode, Encode, decode_from_slice, encode_to_vec};
use redb::{
    Database, ReadTransaction, ReadableTable, TableDefinition, TypeName, Value as RedbValue,
    WriteTransaction,
};
use serde::Serialize;
use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::{self, Field, Schema, Value as TantivyValue},
};
use xxhash_rust::xxh3::xxh3_64;

use crate::error::Error;

#[derive(Debug, Decode, Encode, PartialEq, Clone)]
pub struct FileStateEntry {
    epoch: u128,
    hash: u64,
}

#[derive(Debug, Serialize)]
pub struct FileDocumentEntry {
    pub path: String,
    pub epoch: u128,
    pub hash: u64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchEntry {
    pub path: String,
    pub score: f32,
    pub fragments: HashMap<String, Vec<Range<usize>>>,
}

const DB_FILENAME: &str = "file_states.redb";
const STATE_TABLE: TableDefinition<&str, Bincode<FileStateEntry>> =
    TableDefinition::new("file_states");

pub struct FileSearchReadTransaction {
    txn: ReadTransaction,
    reader: IndexReader,
    field_path: Field,
    field_content: Field,
}

impl FileSearchReadTransaction {
    pub fn new(
        txn: ReadTransaction,
        reader: IndexReader,
        field_path: Field,
        field_content: Field,
    ) -> Self {
        Self {
            txn,
            reader,
            field_path,
            field_content,
        }
    }

    pub fn list(&self) -> Result<Vec<FileDocumentEntry>, Error> {
        let table = self.txn.open_table(STATE_TABLE)?;
        let mut result = Vec::new();

        for entry in table.iter()? {
            let (key_guard, value_guard) = entry?;
            let path = key_guard.value();
            let value = value_guard.value();
            let doc = FileDocumentEntry {
                path: path.into(),
                epoch: value.epoch,
                hash: value.hash,
            };

            result.push(doc);
        }

        Ok(result)
    }

    pub fn search(&self, query: &str, limit: Option<usize>) -> Result<Vec<FileSearchEntry>, Error> {
        let searcher = self.reader.searcher();
        let index = searcher.index();
        let query_parser = QueryParser::for_index(index, vec![self.field_content]);
        let query = query_parser.parse_query(query)?;
        let mut terms = HashSet::new();

        query.query_terms(&mut |term, _| {
            if let Some(text) = term.value().as_str() {
                terms.insert(text.to_string());
            }
        });

        let collector = TopDocs::with_limit(limit.unwrap_or(100_000));
        let top_docs = searcher.search(&query, &collector)?;
        let mut entries = Vec::new();

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            let mut fragments: HashMap<String, Vec<Range<usize>>> = HashMap::new();

            if let Some(content) = Self::get_doc_value(&doc, self.field_content) {
                let mut tokenizer = index.tokenizer_for_field(self.field_content)?;
                let mut token_stream = tokenizer.token_stream(content);

                while let Some(token) = token_stream.next() {
                    let token_text = token.text.to_lowercase();

                    if terms.contains(&token_text) {
                        fragments
                            .entry(token_text)
                            .or_default()
                            .push(token.offset_from..token.offset_to);
                    }
                }
            }

            if let Some(path) = Self::get_doc_value(&doc, self.field_path) {
                entries.push(FileSearchEntry {
                    score,
                    fragments,
                    path: path.into(),
                });
            }
        }

        Ok(entries)
    }

    fn get_doc_value(doc: &TantivyDocument, field: Field) -> Option<&str> {
        doc.get_first(field).and_then(|value| value.as_str())
    }
}

pub struct FileSearchWriteTransaction {
    txn: WriteTransaction,
    writer: IndexWriter<TantivyDocument>,
    field_path: Field,
    field_content: Field,
}

impl FileSearchWriteTransaction {
    pub fn new(
        txn: WriteTransaction,
        writer: IndexWriter<TantivyDocument>,
        field_path: Field,
        field_content: Field,
    ) -> Self {
        Self {
            txn,
            writer,
            field_path,
            field_content,
        }
    }

    pub fn add(&mut self, path: &str) -> Result<(), Error> {
        let epoch = Self::get_file_epoch(path)?;

        match self.get_from_state(path)? {
            Some(state) => {
                if state.epoch != epoch {
                    let (content, hash) = Self::get_file_data(path)?;

                    if state.hash == hash {
                        self.insert_into_state(path, FileStateEntry { epoch, hash })?;
                    } else {
                        self.delete_from_index(path)?;
                        self.insert_into_index(path, content)?;
                        self.insert_into_state(path, FileStateEntry { epoch, hash })?;
                    }
                }
            }
            _ => {
                let (content, hash) = Self::get_file_data(path)?;
                self.insert_into_state(path, FileStateEntry { epoch, hash })?;
                self.insert_into_index(path, content)?;
            }
        }

        Ok(())
    }

    pub fn remove(&mut self, path: &str) -> Result<(), Error> {
        self.delete_from_index(path)?;
        self.delete_from_state(path)
    }

    pub fn clear(&mut self) -> Result<(), Error> {
        let mut table = self.txn.open_table(STATE_TABLE)?;
        let keys: Vec<_> = table
            .iter()?
            .map(|entry| entry.map(|(key, _)| key.value().to_owned()))
            .collect::<Result<_, _>>()?;

        for key in keys.iter() {
            table.remove(key.as_str())?;
        }

        self.writer.delete_all_documents()?;
        Ok(())
    }

    pub fn commit(mut self) -> Result<(), Error> {
        self.writer.commit()?;
        self.txn.commit()?;
        Ok(())
    }

    pub fn rollback(mut self) -> Result<(), Error> {
        self.writer.rollback()?;
        self.txn.abort()?;
        Ok(())
    }

    fn get_file_data(path: &str) -> Result<(String, u64), Error> {
        let content = fs::read_to_string(path)?;
        let hash = xxh3_64(content.as_bytes());
        Ok((content, hash))
    }

    fn get_file_epoch(path: &str) -> Result<u128, Error> {
        Ok(fs::metadata(path)?
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis())
    }

    fn get_from_state(&self, path: &str) -> Result<Option<FileStateEntry>, Error> {
        let table = self.txn.open_table(STATE_TABLE)?;
        Ok(table.get(path)?.map(|entry| entry.value()))
    }

    fn insert_into_state(&mut self, path: &str, entry: FileStateEntry) -> Result<(), Error> {
        let mut table = self.txn.open_table(STATE_TABLE)?;
        table.insert(path, entry)?;
        Ok(())
    }

    fn insert_into_index(&mut self, path: &str, content: String) -> Result<(), Error> {
        let mut document = TantivyDocument::new();
        document.add_field_value(self.field_path, path);
        document.add_field_value(self.field_content, &content);
        self.writer.add_document(document)?;
        Ok(())
    }

    fn delete_from_state(&mut self, path: &str) -> Result<(), Error> {
        let mut table = self.txn.open_table(STATE_TABLE)?;
        table.remove(path)?;
        Ok(())
    }

    fn delete_from_index(&mut self, path: &str) -> Result<(), Error> {
        let term = Term::from_field_text(self.field_path, path);
        self.writer.delete_term(term);
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileSearch {
    db: Database,
    index: Index,
    field_path: Field,
    field_content: Field,
}

impl FileSearch {
    pub fn create(path: &Path) -> Result<Self, Error> {
        let db = Database::create(path.join(DB_FILENAME))?;
        let mut schema_builder = Schema::builder();
        let field_path = schema_builder.add_text_field("path", schema::STRING | schema::STORED);
        let field_content = schema_builder.add_text_field("content", schema::TEXT | schema::STORED);
        let schema = schema_builder.build();
        let dir = MmapDirectory::open(path)?;
        let index = Index::open_or_create(dir, schema)?;

        Ok(Self {
            db,
            index,
            field_path,
            field_content,
        })
    }

    pub fn open_write(&self) -> Result<FileSearchWriteTransaction, Error> {
        Ok(FileSearchWriteTransaction::new(
            self.db.begin_write()?,
            self.index.writer(50_000_000)?,
            self.field_path,
            self.field_content,
        ))
    }

    pub fn open_read(&self) -> Result<FileSearchReadTransaction, Error> {
        Ok(FileSearchReadTransaction::new(
            self.db.begin_read()?,
            self.index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into()?,
            self.field_path,
            self.field_content,
        ))
    }
}

#[derive(Debug)]
struct Bincode<T>(pub T);

impl<T> RedbValue for Bincode<T>
where
    T: Debug + Encode + Decode<()>,
{
    type SelfType<'a>
        = T
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        decode_from_slice(data, bincode::config::standard())
            .unwrap()
            .0
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        encode_to_vec(value, bincode::config::standard()).unwrap()
    }

    fn type_name() -> TypeName {
        TypeName::new(&format!("Bincode<{}>", type_name::<T>()))
    }
}
