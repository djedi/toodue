import test from 'node:test';
import assert from 'node:assert/strict';
import { SCOPES, connectRequestId, isAppRedirect, scopeLabels, summarizeScope } from './oauth.js';

test('read is the only required scope', () => {
  assert.deepEqual(
    SCOPES.filter((s) => s.required).map((s) => s.id),
    ['read']
  );
});

test('scopeLabels keeps canonical order and ignores unknown scopes', () => {
  assert.deepEqual(scopeLabels('delete read bogus'), [
    'View your projects and tasks',
    'Delete tasks'
  ]);
  assert.deepEqual(scopeLabels(['write']), ['Create and update tasks']);
});

test('summarizeScope reports the broadest permission', () => {
  assert.equal(summarizeScope('read'), 'Read only');
  assert.equal(summarizeScope('read write'), 'Read & write');
  assert.equal(summarizeScope('read write delete'), 'Full access');
});

test('connectRequestId only matches the consent route', () => {
  assert.equal(connectRequestId({ pathname: '/connect', search: '?request=abc' }), 'abc');
  assert.equal(connectRequestId({ pathname: '/connect', search: '' }), null);
  assert.equal(connectRequestId({ pathname: '/', search: '?request=abc' }), null);
});

test('isAppRedirect detects desktop app callbacks', () => {
  assert.equal(isAppRedirect('cursor://anysphere.cursor-mcp/oauth/callback?code=x'), true);
  assert.equal(isAppRedirect('https://claude.ai/api/mcp/auth_callback?code=x'), false);
  assert.equal(isAppRedirect('http://127.0.0.1:3000/cb'), false);
});
