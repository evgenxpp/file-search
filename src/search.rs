use std::{
    collections::{HashMap, HashSet},
    fs, io,
    ops::Range,
    path::{Path, PathBuf},
};

use serde::Serialize;
use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, TantivyError, Term,
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::{self, Field, Schema, Value},
};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchEntry {
    pub path: PathBuf,
    pub score: f32,
    pub fragments: HashMap<String, Vec<Range<usize>>>,
}

pub struct FileSearchReadTransaction {
    reader: IndexReader,
    field_path: Field,
    field_content: Field,
}

impl FileSearchReadTransaction {
    pub fn new(reader: IndexReader, field_path: Field, field_content: Field) -> Self {
        Self {
            reader,
            field_path,
            field_content,
        }
    }

    pub fn search(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<FileSearchEntry>, TantivyError> {
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
                let path = Path::new(path);

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
    writer: IndexWriter<TantivyDocument>,
    field_path: Field,
    field_content: Field,
}

impl FileSearchWriteTransaction {
    pub fn new(
        writer: IndexWriter<TantivyDocument>,
        field_path: Field,
        field_content: Field,
    ) -> Self {
        Self {
            writer,
            field_path,
            field_content,
        }
    }

    pub fn append(&mut self, path: &Path) -> Result<(), TantivyError> {
        let path_str = Self::path_to_str(path)?;
        let content = fs::read_to_string(path)?;

        let mut document = TantivyDocument::new();
        document.add_field_value(self.field_path, path_str);
        document.add_field_value(self.field_content, &content);

        let _ = self.writer.add_document(document)?;

        Ok(())
    }

    pub fn remove(&mut self, path: &Path) -> Result<(), TantivyError> {
        let path_str = Self::path_to_str(path)?;
        let term = Term::from_field_text(self.field_path, path_str);
        let _ = self.writer.delete_term(term);

        Ok(())
    }

    pub fn upsert(&mut self, path: &Path) -> Result<(), TantivyError> {
        self.remove(path)?;
        self.append(path)
    }

    pub fn clear(&mut self) -> Result<(), TantivyError> {
        let _ = self.writer.delete_all_documents()?;

        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), TantivyError> {
        let _ = self.writer.commit()?;

        Ok(())
    }

    fn path_to_str(path: &Path) -> Result<&str, io::Error> {
        path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "File path is not valid UTF-8")
        })
    }
}

#[derive(Debug)]
pub struct FileSearch {
    index: Index,
    field_path: Field,
    field_content: Field,
}

impl FileSearch {
    pub fn create(path: &Path) -> Result<Self, TantivyError> {
        let mut schema_builder = Schema::builder();
        let field_path = schema_builder.add_text_field("path", schema::STRING | schema::STORED);
        let field_content = schema_builder.add_text_field("content", schema::TEXT | schema::STORED);
        let schema = schema_builder.build();
        let dir = MmapDirectory::open(path)?;
        let index = Index::open_or_create(dir, schema)?;

        Ok(Self {
            index,
            field_path,
            field_content,
        })
    }

    pub fn open_write(&self) -> Result<FileSearchWriteTransaction, TantivyError> {
        Ok(FileSearchWriteTransaction::new(
            self.index.writer(50_000_000)?,
            self.field_path,
            self.field_content,
        ))
    }

    pub fn open_read(&self) -> Result<FileSearchReadTransaction, TantivyError> {
        Ok(FileSearchReadTransaction::new(
            self.index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into()?,
            self.field_path,
            self.field_content,
        ))
    }
}
