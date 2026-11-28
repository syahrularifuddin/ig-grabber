import assert from 'node:assert/strict';
import test from 'node:test';

import { main, validateReview  } from './llm-review.mjs';

test('accepts structured approval and actionable rejection', () => {
  assert.equal(validateReview({ decision: 'approve', summary: 'No defects found', findings: [] }, []).decision, 'approve');
  assert.equal(validateReview({ decision: 'request_changes', summary: 'Bug', findings: [{ path: 'a.js', severity: 'high', description: 'Null dereference' }] }, ['a.js']).findings.length, 1);
});
test('fails closed on malformed or contradictory reviews', () => {
  for (const value of [null, {}, { decision: 'approve', summary: 'ok', findings: [{ path: 'a.js', severity: 'high', description: 'bug' }] }, { decision: 'request_changes', summary: 'bug', findings: [] }, { decision: 'request_changes', summary: 'bug', findings: [{ path: 'unknown', severity: 'high', description: 'bug' }] }]) {
    assert.throws(() => validateReview(value, ['a.js']));
  }
});

test('status gate uses separate credentials and fails closed on invalid CI or status errors', async() => {
  const saved = { ...process.env };
  const originalFetch = globalThis.fetch;
  const sha = 'a'.repeat(40);
  Object.assign(process.env, { GH_TOKEN: 'app', STATUS_TOKEN: 'status', LLM_API_KEY: 'key', REPOSITORY: 'owner/repo', PR_NUMBER: '9', EXPECTED_SHA: sha });
  delete process.env.WORKFLOW_RUN_ID;
  try {
    for (const brokenStatus of [false, true]) {
      const writes = [];
      globalThis.fetch = async(url, options) => {
        if (url.includes('/statuses/')) {
          assert.ok(url.endsWith(sha));
          assert.equal(options.headers.Authorization, 'Bearer status');
          writes.push(JSON.parse(options.body));
          return { ok: !brokenStatus, status: 403, json: async() => ({}) };
        }
        assert.equal(options.headers.Authorization, 'Bearer app');
        if (url.includes('/runs?')) assert.ok(url.includes('/workflows/ci.yml/'));
        return { ok: true, json: async() => url.includes('/runs?')
          ? { workflow_runs: [{ id: 1, head_sha: sha, status: 'completed', conclusion: 'failure', pull_requests: [{ number: 9 }] }] }
          : { state: 'open', draft: false, head: { sha }, base: { repo: { full_name: 'owner/repo' } } } };
      };
      await assert.rejects(main(), brokenStatus ? /GitHub API failed/ : /lacks successful CI/);
      assert.deepEqual(writes.map(s => s.state), ['pending', 'failure']);
      assert.ok(writes.every(s => s.context === 'AI Review'));
    }
  } finally {
    globalThis.fetch = originalFetch;
    process.env = saved;
  }
});
