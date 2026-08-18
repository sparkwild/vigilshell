# Documentation Standards

## Ownership

`docs/` contains only confirmed, project-level material that will remain useful
for later implementation, review, or maintenance. Each document has one primary
owner in Overview, Features, Technology, Architecture, or Development. Link to
cross-category rules instead of duplicating them.

## Maintenance Rules

- `docs/INDEX.md` is the sole root Markdown document and routes to the five
  category indexes.
- Each category index has exactly `Belongs Here`, `Does Not Belong Here`, and
  `Current Documents` sections, in that order.
- Register every non-index Markdown document once in its owning category index.
- Split a document only when durable sections have independent owners and will
  change for different reasons.
- Use stable concept names rather than dates, phases, tasks, branches, or status
  in durable filenames.

## Working Material Boundary

Plans, milestones, progress reports, status snapshots, TODO lists, research,
review evidence, handoffs, and acceptance output are not durable documentation.
Extract confirmed facts into the relevant owner and keep their originals in a
work-management location. Do not create another `docs/` category for them.
