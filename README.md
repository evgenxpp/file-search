# file-search

📂 A CLI tool written in Rust for full-text search over local files, controlled via interactive stdin commands.

## 🚀 Features

- Add, update, and remove documents from the index
- Clear all indexed data
- Perform fast full-text search with match highlights
- Transactional control via `commit` and `rollback`
- Simple interactive shell over stdin

## 🛠️ Build

```bash
cargo build --release
```