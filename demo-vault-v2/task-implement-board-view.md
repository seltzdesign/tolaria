---
type: task
status: In progress
priority: P1
due: 2026-06-15
start: 2026-05-14
project: "[[Q2 Launch]]"
assignee:
  - "[[person-luca-rossi]]"
labels:
  - frontend
  - editor
estimate: 8
---

# Implement board view for tasks

Group tasks by status into a Kanban-style board. Drag and drop between columns updates the `status` frontmatter on disk through the existing mutation pipeline.

## Acceptance

- Board view renders one column per project status
- Drag-and-drop persists status changes
- Empty columns stay visible with a 0 count
