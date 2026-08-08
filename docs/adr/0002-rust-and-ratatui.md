# Rust and Ratatui for the terminal UI

Vocab is written in Rust using Ratatui for the interface, `rusqlite` for storage, `nucleo` for fuzzy search, and `tokio` for background AI calls.

The tool is launched constantly and briefly — mid-page, while reading — so startup latency is felt directly. A Rust binary starts in single-digit milliseconds against roughly 150ms for a Node/Ink equivalent, and ships as one file with the bundled WordNet database and no runtime to install. The alternative considered was Node + Ink, which is what Claude Code itself is built on and would have made the resemblance free, but it loses on both startup time and distribution.

The cost is that the UI is more code than the React-style equivalent would be.
