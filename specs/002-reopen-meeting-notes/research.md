# Research

- **Decision**: Match event ID plus start instant.
  **Rationale**: Titles repeat, and IDs may identify a recurring series.
  **Alternatives**: Title-only and ID-only matching can mix occurrences.
- **Decision**: Reuse locked yaml-rust2 for front matter.
  **Rationale**: Captures may be edited in Obsidian; quoted strings, sequences and
  timestamp offsets need real parsing. Preserve original text instead of
  serializing the whole document.
  **Alternatives**: String-only parsing is unreliable; a new parser adds dependency cost.
- **Decision**: Reopen schema-4 editable Notes; refuse unsupported layouts explicitly.
  **Rationale**: Schema-3 literal wrappers are not ordinary editable Markdown.
  **Alternatives**: Automatic migration would enlarge scope and risk user edits.
- **Decision**: Background discovery, generation-guarded completion.
  **Rationale**: OneDrive hydration can be slow and choices can change while waiting.
  **Alternatives**: Blocking scans freeze the capture UI.
- **Decision**: Compare restored content before saving or deleting.
  **Rationale**: Checking only note identity cannot detect external vault edits.
  **Alternatives**: Blind regeneration destroys Summary, Follow-ups and properties.
