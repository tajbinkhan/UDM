@AGENTS.md

# Claude-specific notes
- AGENTS.md is the source of truth for architecture and coding rules. Follow it for every task.
- If AGENTS.md and any other instruction conflict, follow AGENTS.md and point out the conflict.
- Never edit architecture rules here; change them in AGENTS.md only.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for tajbinkhan/UDM, through the gh CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels, each named after its role (needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
