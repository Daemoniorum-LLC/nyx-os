# ZSH Aliases

```bash
pf               # cd to persona-framework root
lev-clean        # Clean mammon + leviathan builds
lev-build        # Build leviathan (-x test)
lev-run          # Export AWS creds + bootRun
lev-kill         # Kill all leviathan processes
lev-restart      # Full restart: kill + clean + build + run
lev-logs         # tail -f leviathan.out
bael-dev         # cd bael && npm run dev
bael-test        # cd bael && playwright headed mode
```

# Grimoire Integration

Prompts MUST be loaded from `/home/lilith/development/projects/grimoire/personas/` at runtime.
PromptLoader configured to read from filesystem, NOT packaged JARs.

# Common Workflows

Build + Run Leviathan:
```bash
pf && lev-clean && lev-build && lev-run
```

Run E2E Test:
```bash
pf && cd bael && npx playwright test hydra-marketing-task.e2e.spec.ts:18 --headed --project=chromium
```
