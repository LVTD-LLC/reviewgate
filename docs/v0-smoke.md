# Fresh v0 Smoke Test

Use this after publishing a release and moving the `v0` major tag. Do not rely on rerunning an old workflow run, because GitHub can reuse the action checkout from the old tag resolution.

## Goal

Prove a fresh consumer workflow resolves `LVTD-LLC/reviewgate@v0` to the newly moved tag and publishes the expected concise summary plus inline comments.

## Procedure

1. Move and push the `v0` tag after the release tag is published.
2. In a consumer repository, create a new commit after the tag move. A doc-only change is enough if the workflow supports `mock_artifact`; otherwise use a small PR with a real diff.
3. Run a workflow that uses:

```yaml
- uses: LVTD-LLC/reviewgate@v0
  with:
    openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
    preset: balanced
    summary_style: concise
```

4. Confirm the workflow logs show the action checkout for the new release SHA, not the previous `v0` target.
5. Confirm the PR gets one `ReviewGate: running` placeholder that is replaced by one concise `<!-- reviewgate-summary -->` comment.
6. Confirm the summary shows the score, compact verdict/status line, one-line cost such as `Cost: $0.08 (1 run)`, and no default Metrics, Blocking Findings, Non-Blocking Notes, or Agent Instructions sections.
7. Confirm eligible findings publish as inline PR comments with `<!-- reviewgate-finding:... -->` markers and do not duplicate on a fresh rerun.
8. Record the consumer repo, PR, workflow run URL, resolved action SHA, and any blockers in the release notes or dogfood log.
