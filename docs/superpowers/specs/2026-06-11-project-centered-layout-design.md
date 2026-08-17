# Project-Centered Layout Redesign

## Context

AI Profile Manager started as a profile/environment manager. The desktop app later added project management, sessions, skills, plugins, agents, hooks, usage, terminals, and file browsing. These capabilities now exist, but the product still presents them as separate management modules.

The target change is a layout and information architecture migration: make the project the primary working context while preserving every existing management capability.

## Goals

- Make the selected project the center of the desktop experience.
- Preserve all existing profile, skill, plugin, agent, hook, command, usage, terminal, and project functions.
- Move global management surfaces into toolbar drawers instead of removing them.
- Keep the existing top toolbar controls and terminal behavior.
- Clearly separate global resources from project-level inheritance and overrides.

## Non-Goals

- Do not remove any existing feature.
- Do not rewrite profile, skill, hook, or terminal backends as part of the layout migration.
- Do not merge the right and bottom terminal panels.
- Do not make profiles project-owned. Profiles remain global and reusable across projects.
- Do not mix global resource CRUD with project-level resource override workflows.

## Product Model

The app has three levels:

1. **Global app shell**
   - Top toolbar.
   - Current project indicator.
   - Global drawers for Profiles and Resources.
   - Bottom status bar.
   - Right and bottom terminal panels.

2. **Project workspace**
   - Project navigator on the left.
   - Current project overview and project-specific tabs in the center.
   - Shows what the project uses, inherits, overrides, and has recently done.

3. **Global resource drawers**
   - Profiles drawer for cross-project profile management.
   - Resources drawer for global skills, plugins, agents, hooks, and commands.

## Global Project Context

`activeProject` becomes an app-level context, not only a Projects page selection.

The active project is shown in:

- Toolbar left side: compact project chip, for example `Project: kn · deepseek`.
- Status bar: append current project information while preserving existing status items.
- Project navigator: selected row.
- Right terminal tabs: each AI session tab should carry project metadata.

When a user activates a different project workspace tab, selects a project, resumes a session from another project, or focuses a right terminal tab tied to another project, the toolbar and status bar must update to that project.

The bottom terminal does not automatically change working directory when project context changes. It remains a general-purpose terminal, but the status bar still shows the active project.

## Top Toolbar

The current toolbar controls remain functionally unchanged:

- Quick switcher.
- History.
- Sidebar toggle.
- Bottom terminal toggle.
- Right terminal toggle.
- Environment health.
- Theme and color controls.
- Settings/about/update menu.

Add two global management buttons on the left side near the project chip:

- `Profiles`
- `Resources`

These buttons open large drawers. They do not replace existing toolbar behavior.

## Terminal Behavior

Keep both terminals:

- **Right terminal**
  - AI session workspace.
  - Project `Run default`, profile run, and session resume open here.
  - Tabs should preserve `projectName`, `projectPath`, `profileName`, and command metadata.

- **Bottom terminal**
  - General-purpose command panel.
  - Environment repair, install commands, manual shell commands, and temporary commands open here.
  - Does not auto-switch cwd on project change.

Existing resize, maximize, split-pane, history, search, theme, and close behavior must remain.

## Project Workspace

The main content area becomes a project workspace:

- Left: Project Navigator.
- Center: current project workspace.
- Right terminal and bottom terminal stay as existing shell panels.

Project workspace tabs:

- `Overview`
- `Sessions`
- `Project Skills`
- `Project Agents`
- `Project Hooks`
- `Files`
- `Usage`

Profiles are not a project tab because profiles are global cross-project resources. The project can display and change its default profile, but complete profile management lives in the Profiles drawer.

## Overview

Overview is for decision-making and shortcuts, not deep CRUD.

It should include:

- Current project name and path.
- Default profile summary and quick change.
- Recent sessions.
- Local CLI usage block.
- Resource inheritance/override summary.
- Recommended actions or health warnings.

### Local CLI Usage Block

Show a compact row per CLI:

- CLI name: Claude, Codex, Qoder.
- Installed/missing status.
- Version.
- Usage count.
- Session count.
- Token usage.
- Last used time.

Use compact horizontal bars per statistic rather than large metric cards. Clicking a CLI row can navigate to a filtered `Usage` or `Sessions` view.

## Profiles Drawer

Profiles are global and reusable across projects. The Profiles drawer replaces the old profile Activity surface while preserving all profile functionality.

The drawer opens from the toolbar.

Required migrated functionality:

- Profile list.
- Search.
- Tags/filtering/sorting.
- Multi-select.
- New profile.
- Copy profile.
- Rename profile.
- Delete profile.
- Batch delete.
- Import from file.
- Scan system configs.
- Export single profile.
- Batch export.
- Set global default profile.
- Backup config.
- Restore config.
- Refresh config.
- Environment variable table.
- Add/edit/delete env vars.
- Masked secrets.
- Tags editing.
- Run profile in current project.
- Show usage/history related to the selected profile.
- Show projects that use the selected profile as default or recent profile.

Project default profile selection can be surfaced in the project Overview, but the full profile CRUD remains in this drawer.

## Resources Drawer

Resources are global user-level capabilities. The Resources drawer replaces the old global Skills/Plugins/Hooks activity surfaces while preserving all global resource functionality.

The drawer opens from the toolbar.

Categories:

- Plugins.
- Skills.
- Agents.
- Hooks.
- Commands.

Required migrated functionality:

- Search.
- CLI filtering.
- Status filtering.
- Enable/disable.
- Batch enable/disable.
- Delete/uninstall.
- Batch uninstall.
- Plugin update check.
- Plugin update.
- Marketplace browser.
- Dependency graph.
- Skill/plugin detail views.
- Agent detail views.
- Command detail views.
- File/tree detail views where currently available.
- Hook list grouped by event type.
- Hook detail.
- Hook create wizard.
- Hook store.
- Hook metadata.
- Hook logs if available in the existing detail surface.

The Resources drawer is global-only. It should not show project scope tabs as the primary model.

Project-level resource operations remain in the project workspace:

- Copy/move global resources into the current project.
- Show inherited user-level resources.
- Show project-level overrides.
- Show conflicts.
- Resolve project-specific resource conflicts.

## Project-Level Resource Tabs

Project resource tabs show the relationship between global resources and the current project.

They should support:

- Inherited global resources.
- Project-level resources.
- Overrides.
- Conflicts.
- Copy global resource to project.
- Move/copy project resource back to global where existing behavior supports it.
- Enable/disable project-level resource where supported.
- Delete project-level resource where supported.

These tabs are not a replacement for global resource management. They answer: "What does this project use?"

## Data and State Changes

Recommended frontend structure:

- Introduce `ProjectContext` or equivalent app-level state around `activeProject`.
- Add project metadata to right terminal session/tab records.
- Add toolbar project chip component.
- Extend status bar props with project information.
- Add `ProfileDrawer`.
- Add `ResourceDrawer`.
- Convert old profile sidebar/main panel composition into drawer content.
- Convert old global skills/hooks surfaces into drawer content with project scope removed or hidden.
- Keep project-specific resource management in workspace tabs.

Backend changes should be minimal in the first phase. Existing commands such as project listing, session scanning, skill scanning, hook scanning, profile CRUD, and usage collection should be reused.

## Migration Strategy

### Phase 1: Shell and Context

- Add app-level active project context.
- Show project chip in toolbar.
- Extend status bar with active project.
- Preserve all existing toolbar controls.
- Preserve both terminals.
- Default main layout to project workspace.

### Phase 2: Profiles Drawer

- Move existing profile list and detail management into a large drawer.
- Keep every existing profile operation.
- Add "Run in current project".
- Add "used by projects" summary if data is available.

### Phase 3: Resources Drawer

- Move global Skills/Plugins/Agents/Hooks/Commands management into a Resources drawer.
- Keep all existing global operations.
- Remove project-scope controls from this global drawer.

### Phase 4: Project Workspace Tabs

- Implement Overview with compact CLI usage.
- Implement Sessions, Project Skills, Project Agents, Project Hooks, Files, and Usage tabs.
- Show inherited/global vs project-level resources clearly.
- Keep project-specific copy/move/override workflows.

### Phase 5: Cleanup

- Remove old ActivityBar entries only after their functions are fully reachable from drawers/workspace.
- Keep compatibility during migration by allowing old routes/components to remain internally reused.
- Add tests around active project synchronization and drawer operations.

## Open Implementation Notes

- Current `App.tsx` is carrying too much cross-module orchestration. The migration should extract drawer state, project context, terminal project metadata, and resource scanning hooks gradually.
- Avoid one large rewrite. Prefer wrapping existing components in new layout containers first.
- Do not change storage formats unless a specific feature requires it.
- Keep current user/project scan semantics, but present them more clearly.

