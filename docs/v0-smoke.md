# Fresh v0 Smoke Test

Use this after publishing a release and moving the `v0` major tag. Do not rely on rerunning an old workflow run, because GitHub can reuse the action checkout from the old tag resolution.

## Goal

Prove a fresh consumer workflow resolves `LVTD-LLC/reviewgate@v0` to the newly moved tag and publishes the expected concise summary plus inline finding comments.

## Procedure

1. Push the exact release tag, wait for the runtime workflow to publish its verified release, then move and push `v0`.
2. In a consumer repository, create a small PR with a real diff after the tag move.
3. Run a workflow that uses:

```yaml
- uses: LVTD-LLC/reviewgate@v0
  with:
    openrouter_api_key: ${{ secrets.OPENROUTER_API_KEY }}
    min_severity: P4
```

4. Confirm the workflow logs show the action checkout for the new release SHA, not the previous `v0` target.
5. Confirm runtime installation downloads the pinned release asset, verifies its build-provenance attestation, performs no source compilation, and completes within 15 seconds excluding queue time.
6. Confirm the PR gets one `ReviewGate: running` placeholder that is replaced by one concise `<!-- reviewgate-summary -->` comment.
7. Confirm the summary shows the score, compact verdict, compact finding counts, queue/startup/model/publish durations, footer cost, and no default Metrics, Blocking Findings, Non-Blocking Notes, fallback findings, or Agent Instructions sections.
8. Confirm eligible findings publish as inline PR comments with `<!-- reviewgate-finding:... -->` markers, unanchored/file/PR findings attach to fallback right-side diff line anchors, old standalone comments with `<!-- reviewgate-finding-comment:... -->` markers are cleaned up, and neither duplicates on a fresh rerun.
9. Record the consumer repo, PR, workflow run URL, resolved action SHA, runtime startup duration, and any blockers in the release notes or dogfood log.
