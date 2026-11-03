# Open

Work that is known and not yet done, for whoever picks it up next. An
item leaves this list in the change that finishes it.

- **A release.** `CHANGELOG.md` has an `Unreleased` section (a number is
  shown as its source spells it, where the spelling is JSON's). Cutting
  it, and whether it is 0.6.1 or 0.7.0, is the maintainer's call;
  [RELEASING.md](../RELEASING.md) has the steps.
- **The remote branches** (aless#29). The heads of merged pull requests
  lose nothing when deleted, each still reachable as `refs/pull/N/head`;
  `wip/abnf-grammars` is in `main`'s history. From a fresh fetch:

  ```bash
  git fetch --prune origin
  gh pr list --state merged --limit 500 --json headRefName \
    --jq '.[].headRefName' | sort -u > /tmp/merged
  git branch -r --format='%(refname:lstrip=3)' | sort > /tmp/remote
  comm -12 /tmp/merged /tmp/remote | xargs git push origin --delete
  git push origin --delete wip/abnf-grammars
  ```

  An open pull request's branch is not in the merged list.
  `claude/handover-2026-10-02` (aless#41, closed unmerged, documentation
  only) is the maintainer's call. "Automatically delete head branches"
  in the repository's settings keeps them from piling up again.
- **The Codex review thread on aless#99**, about a CI step that aless#102
  replaced, can be resolved.
