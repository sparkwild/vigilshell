# Documentation Standards

## Documentation Surfaces

Public project presentation belongs in the root `README.md`. The root `AGENTS.md`
is a compact bootstrap and routing surface for coding Agents. Detailed,
confirmed project context belongs under `docs/`; neither root surface should
duplicate the complete product, architecture, or development corpus.

When translations are introduced, the root README remains canonical and locale
variants live under `docs/readme/README.<locale>.md` with reciprocal language
links. README-only media belongs under `docs/assets/readme/`; do not reuse a
runtime application asset merely to avoid creating an appropriate presentation
asset.

The existing Overview, Features, Technology, Architecture, and Development
categories are the project's current navigation structure. Preserve them while
they remain useful, but do not treat them as a fixed taxonomy or create a new
category, index, or placeholder document merely for structural symmetry.

## Ownership And Accuracy

- Give each detailed fact one owning document. Indexes and summaries link to that
  owner instead of restating the rule.
- Keep confirmed product intent distinct from implemented availability. A
  documented feature or Capability is not evidence that its production handler,
  registration gates, tests, or release qualification are complete.
- Verify implementation claims against current manifests, executable
  configuration, source, or tests. Prefer links to those sources for volatile
  versions and commands; record a concrete value only when the project has
  deliberately frozen it.
- Remove or correct obsolete statements instead of retaining contradictory
  history in an active document.
- Do not include developer-local absolute paths, credentials, real endpoints,
  tokens, or sensitive output.

## Structure And Lifecycle

- Preserve established naming and layout unless a reviewed change makes the
  documentation easier to navigate or gives a fact a clearer owner.
- Do not move or delete an existing document without explicit approval for the
  affected paths. Repair all affected routes and links in the same change after
  approval.
- Create a document only when it has confirmed content that cannot remain clear
  in an existing owner. Do not create speculative or empty architecture,
  development, feature, or index files.
- Keep `docs/INDEX.md` as the root navigation entrypoint and register every
  non-index Markdown document once in its owning category index.
- Split a document only when durable sections have independent owners and will
  change for different reasons.
- Use stable concept names rather than dates, phases, tasks, branches, or status
  in durable filenames.

## Working Material Boundary

Plans, milestones, progress reports, status snapshots, TODO lists, research,
review evidence, handoffs, and acceptance output are not durable documentation.
Extract confirmed facts into the relevant owner and keep their originals in a
work-management location. Do not create another `docs/` category for them.

## Validation

For every documentation change:

1. Confirm the changed fact is in its primary owner and is not needlessly
   duplicated elsewhere.
2. Verify changed claims against the repository sources used for the update.
3. Resolve every changed local link and media path, and keep category-index
   registration accurate.
4. Scan changed documents for developer-local paths and task-lifecycle material.
5. Report the documentation surface handled, files changed, sources used,
   checks run, and anything intentionally left uncertain or out of scope.
