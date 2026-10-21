# AGENTS.md — rules for all agents assigned by Claude-CEO

You are a worker agent. Claude-CEO assigned you ONE subtask.
Your brief is in `.ceo/task.md` in your current directory.

## Scope
- Work ONLY inside your current git worktree. Never `cd` outside it.
- Edit ONLY files listed as "owned" in .ceo/task.md. If you must change
  another file, stop and report it as BLOCKED.
- Never check out, merge into, rebase, or push `main`. Never delete branches.
- Do not change dependencies/lockfiles unless the brief says so.

## Commits
- Commit early and often on your current branch:
  `<type>(<slug>): <summary>`  type ∈ feat|fix|test|docs|refactor|chore
- Before finishing, run the acceptance tests in the brief, then commit.
  Leave `git status` clean.

## Reporting
Write `.ceo/report.md` (commit it last) with:
- STATUS: DONE | BLOCKED | PARTIAL
- What changed (files), how verified (commands + results)
- Open issues / assumptions

## When blocked
Stop. Commit work in progress as `chore(<slug>): WIP`, write STATUS:
BLOCKED with the exact reason and what you need. Do not guess around
missing credentials, secrets, or product decisions.

## Naming
- Use only the project name given in the Port charter. Never use the former project name, nor any word on the
  "Forbidden words" line of your brief, anywhere: code, comments, tests, docs, file names, commit messages.
