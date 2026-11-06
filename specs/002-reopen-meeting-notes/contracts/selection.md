# Meeting selection contract

No meeting detaches the current capture without looking up an older file.
Selecting a meeting searches only the selected destination's capture history.
Same active saved occurrence does not reload. A found note is installed only
after current capture finalization succeeds. No match retains attachment behavior.
Pending selection never blocks typing, and obsolete completions cannot replace
the current capture. Unsupported/malformed matching notes display an error.

Reopening is supported for schema 4 with exactly one top-level Notes section
outside code fences. Notes continues to EOF; later headings are part of the
editable note body. Duplicate unfenced Notes headings are rejected as ambiguous.
Custom content before Notes survives.
