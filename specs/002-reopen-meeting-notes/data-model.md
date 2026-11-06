# Data model

- Occurrence key: nonempty calendar event ID and optional UTC-equivalent start.
- Restored capture: original UUID, captured/updated times, timezone, source,
  author, tags, Notes body, selected meeting and original path.
- Stored document: original bytes/text, prefix through Notes heading, original
  meeting and author. Retained to patch Notes and changed managed metadata without
  regenerating other sections; snapshot advances only after successful save.
- Discovery result: newest eligible capture or no match; failures are explicit.
- Pending selection: generation, originating capture ID, destination and selected
  meeting. A completion is ignored after a newer choice, closed/new capture or
  destination change. Current text is saved before installing a result.

Reopened files retain their names. Empty reopened documents are retained rather
than risking deletion of refinements. New captures retain close-only deletion.
